// The Orms HAL service: the onTransact dispatch for all 46
// IOrmsAidlHalService methods plus the reserved getInterfaceVersion /
// getInterfaceHash pair.
//
// Method order is the AIDL transaction-code order (1..46 in declaration
// order, verified against the shipped BnOrmsAidlHalService dispatch and the
// Java stub). Every method returns void except the three readers
// (ormsReadDdrAvailFreq, ormsReadFile, ormsReadGpuFreq), which return a
// String. In-parameters are read in declaration order. The interface token
// is consumed by libbinder before onTransact.

use std::ffi::c_void;

use crate::wire::*;

use orms_service::core;

pub const LOG_TAG: &str = "ORMS_AIDL_HAL";

pub const DESCRIPTOR: &str = "vendor.oplus.hardware.ormsHalService.IOrmsAidlHalService";
pub const INTERFACE_HASH: &str = "45b9fba87d14d35c461848d6bb6cc13324f947c3";
pub const INTERFACE_VERSION: i32 = 1;

pub struct Service {
    inner: core::Service,
}

impl Service {
    pub fn new() -> Self {
        Service {
            inner: core::Service::new(),
        }
    }
}

pub unsafe extern "C" fn on_create(_args: *mut c_void) -> *mut c_void {
    let svc = Service::new();
    log(
        LOG_LEVEL_INFO,
        LOG_TAG,
        &format!("new OrmsAidlHalService.{}", svc.inner.kernel_version()),
    );
    Box::into_raw(Box::new(svc)) as *mut c_void
}

pub unsafe extern "C" fn on_destroy(user_data: *mut c_void) {
    if !user_data.is_null() {
        drop(Box::from_raw(user_data as *mut Service));
    }
}

pub unsafe extern "C" fn on_transact(
    binder: *mut AIBinder,
    code: TransactionCode,
    in_parcel: *const AParcel,
    out_parcel: *mut AParcel,
) -> BinderStatus {
    match dispatch(binder, code, in_parcel, out_parcel) {
        Ok(()) => STATUS_OK,
        Err(rc) => rc,
    }
}

fn dispatch(
    binder: *mut AIBinder,
    code: TransactionCode,
    in_parcel: *const AParcel,
    out_parcel: *mut AParcel,
) -> Result<(), BinderStatus> {
    let svc = unsafe { service_of(binder) };
    match code {
        1 => {
            let pl_handle = read_i32(in_parcel)?;
            let boosts_list = read_i32_array(in_parcel)?;
            core::orms_boost_acquire(&svc.inner, pl_handle, &boosts_list);
            write_ok_status(out_parcel)
        }
        2 => {
            let pl_handle = read_i32(in_parcel)?;
            core::orms_boost_release(&svc.inner, pl_handle);
            write_ok_status(out_parcel)
        }
        3 => {
            let value = read_string(in_parcel)?;
            core::orms_enable_cpu_bouncing(&value);
            write_ok_status(out_parcel)
        }
        4 => {
            let cluster = read_i32(in_parcel)?;
            string_reply(out_parcel, core::orms_read_ddr_avail_freq(cluster))
        }
        5 => {
            let path = read_string(in_parcel)?;
            string_reply(out_parcel, core::orms_read_file(&path))
        }
        6 => {
            let pos = read_i32(in_parcel)?;
            string_reply(out_parcel, core::orms_read_gpu_freq(pos))
        }
        7 => {
            let set_or_release = read_i32(in_parcel)?;
            let cluster = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_above_hispeed_delay(set_or_release, cluster, &value);
            write_ok_status(out_parcel)
        }
        8 => {
            let value = read_string(in_parcel)?;
            core::orms_write_bg_cpu_uclamp_min(&value);
            write_ok_status(out_parcel)
        }
        9 => {
            let value = read_string(in_parcel)?;
            core::orms_write_busy_down_thres(&value);
            write_ok_status(out_parcel)
        }
        10 => {
            let value = read_string(in_parcel)?;
            core::orms_write_busy_up_thres(&value);
            write_ok_status(out_parcel)
        }
        11 => {
            let value = read_string(in_parcel)?;
            core::orms_write_camera_tracing_events(&value);
            write_ok_status(out_parcel)
        }
        12 => {
            let value = read_string(in_parcel)?;
            core::orms_write_core_ctl_enable(&value);
            write_ok_status(out_parcel)
        }
        13 => {
            let value = read_string(in_parcel)?;
            core::orms_write_cpu_bouncing(&value);
            write_ok_status(out_parcel)
        }
        14 => {
            let cluster = read_i32(in_parcel)?;
            let min = read_i32(in_parcel)?;
            let max = read_i32(in_parcel)?;
            core::orms_write_cpu_core_num(cluster, min, max);
            write_ok_status(out_parcel)
        }
        15 => {
            let cluster = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_cpu_ddr_latfloor_max(cluster, &value);
            write_ok_status(out_parcel)
        }
        16 => {
            let cluster_offset = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_cpu_ddr_latfloor_max2(cluster_offset, &value);
            write_ok_status(out_parcel)
        }
        17 => {
            let cluster = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_cpu_ddr_latfloor_min(cluster, &value);
            write_ok_status(out_parcel)
        }
        18 => {
            let cluster_offset = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_cpu_ddr_latfloor_min2(cluster_offset, &value);
            write_ok_status(out_parcel)
        }
        19 => {
            let cluster = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_cpu_l3_lat_max(cluster, &value);
            write_ok_status(out_parcel)
        }
        20 => {
            let cluster = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_cpu_llcc_lat_max(cluster, &value);
            write_ok_status(out_parcel)
        }
        21 => {
            let check_mask = read_i32(in_parcel)?;
            let online_mask = read_i32(in_parcel)?;
            core::orms_write_cpu_online(check_mask, online_mask);
            write_ok_status(out_parcel)
        }
        22 => {
            let value = read_string(in_parcel)?;
            core::orms_write_fg_cpu_uclamp_min(&value);
            write_ok_status(out_parcel)
        }
        23 => {
            let value = read_string(in_parcel)?;
            core::orms_write_fpsgo(&value);
            write_ok_status(out_parcel)
        }
        24 => {
            let value = read_string(in_parcel)?;
            core::orms_write_hwmon_hyst_opt(&value);
            write_ok_status(out_parcel)
        }
        25 => {
            let value = read_string(in_parcel)?;
            core::orms_write_input_boost_enabled(&svc.inner, &value);
            write_ok_status(out_parcel)
        }
        26 => {
            let value = read_string(in_parcel)?;
            core::orms_write_input_boost_freq(&value);
            write_ok_status(out_parcel)
        }
        27 => {
            let cluster = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_llcc_ddr_lat_max(cluster, &value);
            write_ok_status(out_parcel)
        }
        28 => {
            let value = read_string(in_parcel)?;
            core::orms_write_low_power_mode(&value);
            write_ok_status(out_parcel)
        }
        29 => {
            let value = read_string(in_parcel)?;
            core::orms_write_memlat_l3_opt(&value);
            write_ok_status(out_parcel)
        }
        30 => {
            let value = read_string(in_parcel)?;
            core::orms_write_prefer_silver_enabled(&value);
            write_ok_status(out_parcel)
        }
        31 => {
            let value = read_string(in_parcel)?;
            core::orms_write_ruler_enable(&value);
            write_ok_status(out_parcel)
        }
        32 => {
            let value = read_string(in_parcel)?;
            core::orms_write_schedtune_colocate(&value);
            write_ok_status(out_parcel)
        }
        33 => {
            let value = read_string(in_parcel)?;
            core::orms_write_schedtune_prefer_idle(&value);
            write_ok_status(out_parcel)
        }
        34 => {
            let value = read_string(in_parcel)?;
            core::orms_write_sleep_disabled(&value);
            write_ok_status(out_parcel)
        }
        35 => {
            let value = read_string(in_parcel)?;
            core::orms_write_slide_boost(&svc.inner, &value);
            write_ok_status(out_parcel)
        }
        36 => {
            let set_or_release = read_i32(in_parcel)?;
            let cluster = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_target_loads(set_or_release, cluster, &value);
            write_ok_status(out_parcel)
        }
        37 => {
            let value = read_string(in_parcel)?;
            core::orms_write_top_cpu_uclamp_min(&value);
            write_ok_status(out_parcel)
        }
        38 => {
            let value = read_string(in_parcel)?;
            core::orms_write_touch_boost(&value);
            write_ok_status(out_parcel)
        }
        39 => {
            let value = read_string(in_parcel)?;
            core::orms_write_tracing_set_event(&value);
            write_ok_status(out_parcel)
        }
        40 => {
            let value = read_string(in_parcel)?;
            core::orms_write_uclamp_latency_sensitive(&value);
            write_ok_status(out_parcel)
        }
        41 => {
            let value1 = read_string(in_parcel)?;
            let value2 = read_string(in_parcel)?;
            core::orms_write_hal_ufs_plus_ctrl(&value1, &value2);
            write_ok_status(out_parcel)
        }
        42 => {
            let value = read_string(in_parcel)?;
            core::orms_write_sched_asymcap_boost(&value);
            write_ok_status(out_parcel)
        }
        43 => {
            let cluster = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_cpu_cpu_ddr_lat_min(cluster, &value);
            write_ok_status(out_parcel)
        }
        44 => {
            let value = read_string(in_parcel)?;
            core::orms_write_cpu_cpu_ddr_bw_min(&value);
            write_ok_status(out_parcel)
        }
        45 => {
            let id = read_i32(in_parcel)?;
            let cluster = read_i32(in_parcel)?;
            let value = read_string(in_parcel)?;
            core::orms_write_node_common(id, cluster, &value);
            write_ok_status(out_parcel)
        }
        46 => {
            let value = read_string(in_parcel)?;
            core::orms_write_force_step(&value);
            write_ok_status(out_parcel)
        }
        TRANSACTION_GET_INTERFACE_VERSION => {
            write_ok_status(out_parcel)?;
            write_i32(out_parcel, INTERFACE_VERSION)
        }
        TRANSACTION_GET_INTERFACE_HASH => {
            write_ok_status(out_parcel)?;
            write_string(out_parcel, INTERFACE_HASH)
        }
        _ => return Err(-31), // STATUS_UNKNOWN_TRANSACTION
    }
}

/// The service holds no per-binder state, so the single instance is reached
/// through AIBinder_getUserData.
///
/// # Safety
/// `binder` must be the binder this process created and registered, whose
/// user data points at a live `Service`.
unsafe fn service_of(binder: *mut AIBinder) -> &'static Service {
    let ptr = AIBinder_getUserData(binder);
    &*(ptr as *const Service)
}

fn string_reply(out: *mut AParcel, value: String) -> Result<(), BinderStatus> {
    write_ok_status(out)?;
    write_string(out, &value)
}
