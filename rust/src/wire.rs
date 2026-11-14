// Raw FFI to libbinder_ndk (the stable C API) and parcel helpers.
//
// The AIDL transport is implemented by hand instead of through generated C++
// code: the interface is fixed (vendor.oplus.hardware.ormsHalService.
// IOrmsAidlHalService, version 1, hash
// 45b9fba87d14d35c461848d6bb6cc13324f947c3, 46 methods numbered 1..46 plus
// the reserved pair getInterfaceVersion@0xffffff / getInterfaceHash@0xfffffe
// that this ROM's Java stub uses), so the wire format is fully known and the
// NDK C API is a stable C ABI that Rust can bind directly.
//
// Wire format (matches what the generated ndk backend and the framework's
// Java stub produce):
//   * incoming transactions arrive with the interface token already consumed,
//   * in-parameters are read in declaration order,
//   * every reply starts with a status header (an int32 exception code, 0 for
//     OK); void methods write nothing else, String methods write the string
//     after it,
//   * strings are int32 length (UTF-8 bytes, -1 for null) + bytes + NUL;
//     int arrays are int32 length followed by the elements.

#![allow(dead_code)]

use std::ffi::{c_char, c_void, CString};
use std::os::raw::c_int;
use std::ptr;

#[repr(C)]
pub struct AIBinder {
    _private: [u8; 0],
}
#[repr(C)]
pub struct AIBinder_Class {
    _private: [u8; 0],
}
#[repr(C)]
pub struct AParcel {
    _private: [u8; 0],
}
#[repr(C)]
pub struct AStatus {
    _private: [u8; 0],
}

pub type BinderStatus = c_int;
pub type TransactionCode = u32;

pub const STATUS_OK: BinderStatus = 0;

pub const TRANSACTION_GET_INTERFACE_VERSION: TransactionCode = 16777215;
pub const TRANSACTION_GET_INTERFACE_HASH: TransactionCode = 16777214;

pub const LOG_LEVEL_INFO: c_int = 4;
pub const LOG_LEVEL_ERROR: c_int = 6;

pub type OnCreate = unsafe extern "C" fn(args: *mut c_void) -> *mut c_void;
pub type OnDestroy = unsafe extern "C" fn(user_data: *mut c_void);
pub type OnTransact = unsafe extern "C" fn(
    binder: *mut AIBinder,
    code: TransactionCode,
    in_parcel: *const AParcel,
    out_parcel: *mut AParcel,
) -> BinderStatus;

extern "C" {
    // android/binder_ibinder.h
    pub fn AIBinder_Class_define(
        interface_descriptor: *const c_char,
        on_create: OnCreate,
        on_destroy: OnDestroy,
        on_transact: OnTransact,
    ) -> *mut AIBinder_Class;
    pub fn AIBinder_new(class: *mut AIBinder_Class, args: *mut c_void) -> *mut AIBinder;
    pub fn AIBinder_getUserData(binder: *mut AIBinder) -> *mut c_void;

    // android/binder_manager.h (platform header)
    pub fn AServiceManager_addService(
        binder: *mut AIBinder,
        instance: *const c_char,
    ) -> BinderStatus;

    // android/binder_process.h (platform header)
    pub fn ABinderProcess_setThreadPoolMaxThreadCount(num_threads: u32) -> bool;
    pub fn ABinderProcess_startThreadPool();
    pub fn ABinderProcess_joinThreadPool();

    // android/binder_parcel.h
    pub fn AParcel_writeInt32(parcel: *mut AParcel, value: i32) -> BinderStatus;
    pub fn AParcel_readInt32(parcel: *const AParcel, value: *mut i32) -> BinderStatus;
    pub fn AParcel_writeString(
        parcel: *mut AParcel,
        string: *const c_char,
        length: i32,
    ) -> BinderStatus;
    pub fn AParcel_readString(
        parcel: *const AParcel,
        string_data: *mut c_void,
        allocator: Option<unsafe extern "C" fn(*mut c_void, i32, *mut *mut c_char) -> bool>,
    ) -> BinderStatus;
    pub fn AParcel_readInt32Array(
        parcel: *const AParcel,
        array_data: *mut c_void,
        allocator: Option<unsafe extern "C" fn(*mut c_void, i32, *mut *mut i32) -> bool>,
    ) -> BinderStatus;
    pub fn AParcel_writeStatusHeader(parcel: *mut AParcel, status: *const AStatus) -> BinderStatus;

    // android/binder_status.h
    pub fn AStatus_newOk() -> *mut AStatus;
    pub fn AStatus_delete(status: *mut AStatus);

    // android/log.h
    pub fn __android_log_print(prio: c_int, tag: *const c_char, fmt: *const c_char, ...) -> c_int;
}

pub fn log(level: c_int, tag: &str, msg: &str) {
    let tag = match CString::new(tag) {
        Ok(t) => t,
        Err(_) => return,
    };
    let fmt = CString::new("%s").unwrap();
    let msg = match CString::new(msg) {
        Ok(m) => m,
        Err(_) => return,
    };
    // SAFETY: all three strings are valid NUL-terminated C strings.
    unsafe {
        __android_log_print(level, tag.as_ptr(), fmt.as_ptr(), msg.as_ptr());
    }
}

// Write helpers.

pub fn write_ok_status(out: *mut AParcel) -> Result<(), BinderStatus> {
    // SAFETY: out is a valid reply parcel from onTransact.
    let status = unsafe { AStatus_newOk() };
    if status.is_null() {
        return Err(STATUS_OK - 1);
    }
    let rc = unsafe { AParcel_writeStatusHeader(out, status) };
    unsafe { AStatus_delete(status) };
    if rc == STATUS_OK {
        Ok(())
    } else {
        Err(rc)
    }
}

pub fn write_i32(out: *mut AParcel, value: i32) -> Result<(), BinderStatus> {
    // SAFETY: out is a valid reply parcel.
    let rc = unsafe { AParcel_writeInt32(out, value) };
    if rc == STATUS_OK {
        Ok(())
    } else {
        Err(rc)
    }
}

pub fn write_string(out: *mut AParcel, value: &str) -> Result<(), BinderStatus> {
    let cstr = match CString::new(value) {
        Ok(c) => c,
        Err(_) => return Err(-22), // STATUS_BAD_VALUE
    };
    // SAFETY: out is a valid reply parcel; cstr is NUL-terminated and its
    // length is passed explicitly as the UTF-8 byte count.
    let rc = unsafe { AParcel_writeString(out, cstr.as_ptr(), cstr.as_bytes().len() as i32) };
    if rc == STATUS_OK {
        Ok(())
    } else {
        Err(rc)
    }
}

// Read helpers.

pub fn read_i32(in_parcel: *const AParcel) -> Result<i32, BinderStatus> {
    let mut value: i32 = 0;
    // SAFETY: in_parcel is a valid incoming data parcel; &mut value is a
    // writable i32.
    let rc = unsafe { AParcel_readInt32(in_parcel, &mut value) };
    if rc == STATUS_OK {
        Ok(value)
    } else {
        Err(rc)
    }
}

pub fn read_string(in_parcel: *const AParcel) -> Result<String, BinderStatus> {
    let mut ctx = StringCtx {
        buf: ptr::null_mut(),
        layout: std::alloc::Layout::new::<u8>(),
    };
    // SAFETY: in_parcel is a valid incoming data parcel. The allocator
    // callback leases a buffer that libbinder fills with the string, and the
    // StringCtx Drop frees it when the ctx goes out of scope.
    let rc = unsafe {
        AParcel_readString(
            in_parcel,
            &mut ctx as *mut StringCtx as *mut c_void,
            Some(string_allocator),
        )
    };
    if rc != STATUS_OK {
        return Err(rc);
    }
    if ctx.buf.is_null() {
        // A null string in the parcel reads as empty.
        return Ok(String::new());
    }
    // SAFETY: buf holds layout.size() bytes with the string and its NUL
    // terminator written by libbinder inside them.
    let bytes = unsafe { std::slice::from_raw_parts(ctx.buf, ctx.layout.size()) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

pub fn read_i32_array(in_parcel: *const AParcel) -> Result<Vec<i32>, BinderStatus> {
    let mut ctx = Int32ArrayCtx {
        buf: ptr::null_mut(),
        layout: std::alloc::Layout::new::<i32>(),
    };
    // SAFETY: as with read_string, with i32 elements.
    let rc = unsafe {
        AParcel_readInt32Array(
            in_parcel,
            &mut ctx as *mut Int32ArrayCtx as *mut c_void,
            Some(i32_array_allocator),
        )
    };
    if rc != STATUS_OK {
        return Err(rc);
    }
    if ctx.buf.is_null() {
        // A null or empty array in the parcel reads as empty.
        return Ok(Vec::new());
    }
    // SAFETY: buf holds layout.size() bytes of i32s written by libbinder.
    let values = unsafe { std::slice::from_raw_parts(ctx.buf, ctx.layout.size() / 4) };
    Ok(values.to_vec())
}

/// A binder transaction is capped at 1 MiB, so no larger string is legitimate.
const MAX_PARCEL_STRING: usize = 1 << 20;

struct StringCtx {
    buf: *mut u8,
    layout: std::alloc::Layout,
}

impl Drop for StringCtx {
    fn drop(&mut self) {
        if !self.buf.is_null() {
            // SAFETY: buf was allocated with exactly this layout.
            unsafe { std::alloc::dealloc(self.buf, self.layout) };
        }
    }
}

unsafe extern "C" fn string_allocator(
    data: *mut c_void,
    length: i32,
    buffer: *mut *mut c_char,
) -> bool {
    let ctx = &mut *(data as *mut StringCtx);
    // AParcel_stringAllocator contract (binder_parcel.h): length includes the
    // NUL terminator, and -1 means a null string, which succeeds with no
    // buffer. 0 is documented as never sent; treat it the same way.
    if length <= 0 {
        return true;
    }
    if (length as usize) > MAX_PARCEL_STRING {
        return false;
    }
    let layout = match std::alloc::Layout::from_size_align(length as usize, 1) {
        Ok(l) => l,
        Err(_) => return false,
    };
    let p = std::alloc::alloc(layout);
    if p.is_null() {
        return false;
    }
    ctx.buf = p;
    ctx.layout = layout;
    *buffer = p as *mut c_char;
    true
}

const MAX_PARCEL_INT32_ARRAY: usize = 1 << 18; // 1 MiB of i32 elements

struct Int32ArrayCtx {
    buf: *mut i32,
    layout: std::alloc::Layout,
}

impl Drop for Int32ArrayCtx {
    fn drop(&mut self) {
        if !self.buf.is_null() {
            // SAFETY: buf was allocated with exactly this layout.
            unsafe { std::alloc::dealloc(self.buf as *mut u8, self.layout) };
        }
    }
}

unsafe extern "C" fn i32_array_allocator(
    data: *mut c_void,
    length: i32,
    buffer: *mut *mut i32,
) -> bool {
    let ctx = &mut *(data as *mut Int32ArrayCtx);
    // AParcel_int32ArrayAllocator: -1 means a null array, no buffer needed.
    if length <= 0 {
        return true;
    }
    if (length as usize) > MAX_PARCEL_INT32_ARRAY {
        return false;
    }
    let layout = match std::alloc::Layout::array::<i32>(length as usize) {
        Ok(l) => l,
        Err(_) => return false,
    };
    let p = std::alloc::alloc(layout) as *mut i32;
    if p.is_null() {
        return false;
    }
    ctx.buf = p;
    ctx.layout = layout;
    *buffer = p;
    true
}
