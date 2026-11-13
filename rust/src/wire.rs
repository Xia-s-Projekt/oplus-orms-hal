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
    let mut sink = StringSink::default();
    // SAFETY: in_parcel is a valid incoming data parcel; the allocator copies
    // the bytes out before the call returns.
    let rc = unsafe {
        AParcel_readString(
            in_parcel,
            &mut sink as *mut StringSink as *mut c_void,
            Some(string_allocator),
        )
    };
    if rc == STATUS_OK {
        sink.take()
    } else {
        Err(rc)
    }
}

pub fn read_i32_array(in_parcel: *const AParcel) -> Result<Vec<i32>, BinderStatus> {
    let mut sink = ArraySink::default();
    // SAFETY: as above, with the i32 array allocator.
    let rc = unsafe {
        AParcel_readInt32Array(
            in_parcel,
            &mut sink as *mut ArraySink as *mut c_void,
            Some(i32_array_allocator),
        )
    };
    if rc == STATUS_OK {
        sink.take()
    } else {
        Err(rc)
    }
}

#[derive(Default)]
struct StringSink {
    data: Option<String>,
}

impl StringSink {
    fn take(&mut self) -> Result<String, BinderStatus> {
        match self.data.take() {
            Some(s) => Ok(s),
            None => Err(-22),
        }
    }
}

unsafe extern "C" fn string_allocator(
    data: *mut c_void,
    _length: i32,
    string_data: *mut *mut c_char,
) -> bool {
    let sink = &mut *(data as *mut StringSink);
    match std::ffi::CStr::from_ptr(*string_data).to_str() {
        Ok(text) => {
            sink.data = Some(text.to_string());
            true
        }
        Err(_) => false,
    }
}

#[derive(Default)]
struct ArraySink {
    data: Vec<i32>,
}

impl ArraySink {
    fn take(&mut self) -> Result<Vec<i32>, BinderStatus> {
        Ok(std::mem::take(&mut self.data))
    }
}

unsafe extern "C" fn i32_array_allocator(
    data: *mut c_void,
    length: i32,
    array_data: *mut *mut i32,
) -> bool {
    let sink = &mut *(data as *mut ArraySink);
    let values = std::slice::from_raw_parts(*array_data, length as usize);
    sink.data = values.to_vec();
    true
}
