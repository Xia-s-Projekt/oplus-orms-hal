// OPlus Orms HAL service entry point.
//
// Startup sequence:
//
//   ABinderProcess_setThreadPoolMaxThreadCount(4);
//   ABinderProcess_startThreadPool();
//   instance = "vendor.oplus.hardware.ormsHalService.IOrmsAidlHalService/default";
//   status   = AServiceManager_addService(binder, instance);
//   ABinderProcess_joinThreadPool();
//
// The log tag is "ORMS_AIDL_HAL".

#[path = "../wire.rs"]
mod wire;
#[path = "../hal.rs"]
mod hal;

use std::ffi::CString;
use std::os::raw::c_char;

use hal::{on_create, on_destroy, on_transact, DESCRIPTOR, LOG_TAG};
use wire::{
    log, AIBinder_Class_define, AIBinder_new, ABinderProcess_joinThreadPool,
    ABinderProcess_setThreadPoolMaxThreadCount, ABinderProcess_startThreadPool,
    AServiceManager_addService, LOG_LEVEL_ERROR, LOG_LEVEL_INFO,
};

fn main() {
    // SAFETY: plain binder calls with scalar arguments.
    let rc = unsafe { ABinderProcess_setThreadPoolMaxThreadCount(4) };
    if !rc {
        log(LOG_LEVEL_ERROR, LOG_TAG, "setThreadPoolMaxThreadCount(4) failed");
    }
    // SAFETY: no arguments.
    unsafe { ABinderProcess_startThreadPool() };

    let descriptor = CString::new(DESCRIPTOR).unwrap();
    // SAFETY: descriptor is NUL-terminated; the handlers match the typedefs.
    let class = unsafe {
        AIBinder_Class_define(
            descriptor.as_ptr() as *const c_char,
            on_create,
            on_destroy,
            on_transact,
        )
    };
    if class.is_null() {
        log(LOG_LEVEL_ERROR, LOG_TAG, "AIBinder_Class_define failed");
        std::process::exit(1);
    }

    // SAFETY: class was just defined; on_create builds the service state
    // itself, so no args are needed.
    let binder = unsafe { AIBinder_new(class, std::ptr::null_mut()) };
    if binder.is_null() {
        log(LOG_LEVEL_ERROR, LOG_TAG, "AIBinder_new failed");
        std::process::exit(1);
    }

    // The default instance is a single literal so the exact fqname is
    // statically visible in the binary.
    const DEFAULT_INSTANCE: &str =
        "vendor.oplus.hardware.ormsHalService.IOrmsAidlHalService/default";
    // Diagnostic-only instance override: production always registers the
    // shipped "/default" instance unless central debug is enabled.
    let name = if orms_service::io_shim::debug_enabled() {
        match std::env::var("OPLUS_ORMS_INSTANCE") {
            Ok(suffix) if !suffix.is_empty() => format!("{}/{}", DESCRIPTOR, suffix),
            _ => DEFAULT_INSTANCE.to_string(),
        }
    } else {
        DEFAULT_INSTANCE.to_string()
    };
    let instance = CString::new(name.clone()).unwrap();
    // SAFETY: binder is a live AIBinder we own; instance is NUL-terminated.
    let status = unsafe { AServiceManager_addService(binder, instance.as_ptr() as *const c_char) };
    if status != 0 {
        log(
            LOG_LEVEL_ERROR,
            LOG_TAG,
            &format!("addService({}) -> status {}", name, status),
        );
        std::process::exit(1);
    }
    log(LOG_LEVEL_INFO, LOG_TAG, &format!("registered {}", name));

    // SAFETY: hands the main thread to the binder thread pool; returns only
    // when the process is shutting down.
    unsafe { ABinderProcess_joinThreadPool() };
}
