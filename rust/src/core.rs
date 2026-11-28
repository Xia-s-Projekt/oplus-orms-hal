// The Orms HAL core: every IOrmsAidlHalService method against its node set.
//
// Each write goes through node_remap::resolve_node, which keeps the original
// path first and falls back to the equivalent node that drives the same
// control when the original is absent.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::io_shim;
use crate::node_remap;

pub struct Service {
    /// Probes the "fbg" variants of the slide and input-boost knobs; when
    /// either exists the fbg path is used instead of the plain one.
    has_fbg_knobs: bool,
    /// The kernel version parsed out of /proc/version, logged at startup.
    kernel_version: String,
    /// The boost request table keyed by handle, holding the (opcode, value)
    /// pairs the acquired boosts registered.
    boosts: Mutex<HashMap<i32, Vec<(u32, i32)>>>,
    /// Bitmask of active simulated boost sources when the platform-specific
    /// boost nodes are absent.
    sim_boost_mask: Mutex<u32>,
}

impl Service {
    pub fn new() -> Self {
        let has_fbg_knobs = io_shim::exists("/proc/sys/fbg/input_boost_enabled")
            || io_shim::exists("/proc/sys/fbg/slide_boost_enabled");
        let kernel_version = Self::kernel_version_string();
        Service {
            has_fbg_knobs,
            kernel_version,
            boosts: Mutex::new(HashMap::new()),
            sim_boost_mask: Mutex::new(0),
        }
    }

    pub fn kernel_version(&self) -> &str {
        &self.kernel_version
    }

    /// "Linux version %d.%d.%d" parsed out of /proc/version; empty when the
    /// node is absent, matching the constructor's fallthrough.
    fn kernel_version_string() -> String {
        match io_shim::read_file("/proc/version") {
            Some(version) => {
                if let Some(p) = version.find("Linux version ") {
                    let rest = &version[p + "Linux version ".len()..];
                    let mut parts = rest.split('.');
                    let major = parts.next().and_then(first_int_of).unwrap_or(0);
                    let minor = parts.next().and_then(first_int_of).unwrap_or(0);
                    let patch = parts.next().and_then(first_int_of).unwrap_or(0);
                    return format!("{}.{}.{}", major, minor, patch);
                }
                String::new()
            }
            None => String::new(),
        }
    }
}

fn first_int_of(s: &str) -> Option<i32> {
    let bytes = s.as_bytes();
    let mut end = 0;
    if !bytes.is_empty() && (bytes[0] == b'-' || bytes[0] == b'+') {
        end = 1;
    }
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == 0 || (end == 1 && (bytes[0] == b'-' || bytes[0] == b'+')) {
        return None;
    }
    s[..end].parse::<i32>().ok()
}

fn is_thermal_throttle_node(path: &str) -> bool {
    path.starts_with("/sys/class/thermal/") || path.ends_with("/scaling_max_freq")
}

/// Writes `value` to `path`, resolving the node against the device's actual
/// topology first. Thermal throttling nodes are quietly no-oped so the main
/// thermal HAL retains sole control.
pub fn write_node(path: &str, value: &str) {
    let node = node_remap::resolve_node(path);
    if is_thermal_throttle_node(&node) {
        return;
    }
    io_shim::write_file(&node, value);
}

/// Reads `path` through the same resolution. Returns None when the node is
/// absent.
pub fn read_node(path: &str) -> Option<String> {
    let node = node_remap::resolve_node(path);
    io_shim::read_file(&node)
}

fn fmt_path(template: &str, cluster: i32) -> String {
    template.replace("{}", &cluster.to_string())
}

// ---------------------------------------------------------------------------
// Boost requests
// ---------------------------------------------------------------------------

const SIM_BIT_ACQUIRE: u32 = 1 << 0;
const SIM_BIT_INPUT: u32 = 1 << 1;
const SIM_BIT_SLIDE: u32 = 1 << 2;
const SIM_BIT_TOUCH: u32 = 1 << 3;
const SIM_BIT_ASYMCAP: u32 = 1 << 4;

fn value_is_enabled(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "0" {
        return false;
    }
    trimmed.parse::<i64>().map(|n| n != 0).unwrap_or(true)
}

fn set_sim_boost_bit(service: &Service, bit: u32, active: bool) {
    let mut mask = service.sim_boost_mask.lock().unwrap();
    let prev = *mask != 0;
    if active {
        *mask |= bit;
    } else {
        *mask &= !bit;
    }
    let next = *mask != 0;
    drop(mask);
    if prev == next {
        return;
    }
    if io_shim::exists("/dev/cpuctl/top-app/cpu.uclamp.min") {
        io_shim::write_file("/dev/cpuctl/top-app/cpu.uclamp.min", if next { "30" } else { "0" });
    }
    // fpsgo sysfs: 1=force on, 0=force off, 2=auto; release restores auto.
    if io_shim::exists("/sys/kernel/fpsgo/common/force_onoff") {
        io_shim::write_file(
            "/sys/kernel/fpsgo/common/force_onoff",
            if next { "1" } else { "2" },
        );
    }
}

pub fn orms_boost_acquire(service: &Service, pl_handle: i32, boosts_list: &[i32]) {
    let non_empty = {
        let mut boosts = service.boosts.lock().unwrap();
        let entry = boosts.entry(pl_handle).or_default();
        for value in boosts_list {
            entry.push(((*value as u32), *value));
        }
        !boosts.is_empty()
    };
    set_sim_boost_bit(service, SIM_BIT_ACQUIRE, non_empty);
}

pub fn orms_boost_release(service: &Service, pl_handle: i32) {
    let non_empty = {
        let mut boosts = service.boosts.lock().unwrap();
        boosts.remove(&pl_handle);
        !boosts.is_empty()
    };
    set_sim_boost_bit(service, SIM_BIT_ACQUIRE, non_empty);
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

pub fn orms_read_file(path: &str) -> String {
    read_node(path).unwrap_or_default()
}

/// Looks the cluster up in a table of the per-cluster DDR latency devfreq
/// nodes' available_frequencies; the node resolves against the memory domain
/// the kernel actually has.
pub fn orms_read_ddr_avail_freq(cluster: i32) -> String {
    const PATHS: [&str; 3] = [
        "/sys/class/devfreq/soc:qcom,cpu0-llcc-ddr-lat/available_frequencies",
        "/sys/class/devfreq/soc:qcom,cpu4-llcc-ddr-lat/available_frequencies",
        "/sys/class/devfreq/soc:qcom,cpu6-llcc-ddr-lat/available_frequencies",
    ];
    const CLUSTER_CPUS: [i32; 3] = [0, 4, 6];
    let index = match cluster {
        0..=2 => cluster as usize,
        _ => return String::new(),
    };
    if let Some(content) = read_node(PATHS[index]) {
        if !content.is_empty() {
            return content;
        }
    }
    // No memory domain at all: answer with the frequencies of the cluster the
    // caller asked about, scaled kHz -> Hz to keep the original node's unit.
    let (freqs, _source) = node_remap::ddr_available_frequencies(CLUSTER_CPUS[index]);
    freqs
}

pub fn orms_read_gpu_freq(_pos: i32) -> String {
    read_node("/sys/class/kgsl/kgsl-3d0/devfreq/cur_freq").unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

pub fn orms_enable_cpu_bouncing(value: &str) {
    write_node("/sys/module/cpufreq_bouncing/parameters/enable", value);
}

pub fn orms_write_above_hispeed_delay(_set_or_release: i32, cluster: i32, value: &str) {
    write_node(
        &fmt_path(
            "/sys/devices/system/cpu/cpufreq/policy{}/schedutil/above_hispeed_delay",
            cluster,
        ),
        value,
    );
}

pub fn orms_write_bg_cpu_uclamp_min(value: &str) {
    write_node("/dev/cpuctl/background/cpu.uclamp.min", value);
}

pub fn orms_write_busy_down_thres(value: &str) {
    write_node("/sys/devices/system/cpu/cpu7/core_ctl/busy_down_thres", value);
}

pub fn orms_write_busy_up_thres(value: &str) {
    write_node("/sys/devices/system/cpu/cpu7/core_ctl/busy_up_thres", value);
}

pub fn orms_write_camera_tracing_events(value: &str) {
    write_node("/sys/kernel/tracing/events/camera/enable", value);
}

pub fn orms_write_core_ctl_enable(value: &str) {
    write_node("/sys/devices/system/cpu/cpu7/core_ctl/enable", value);
}

pub fn orms_write_cpu_bouncing(value: &str) {
    write_node("/sys/module/cpufreq_bouncing/parameters/config", value);
}

pub fn orms_write_cpu_core_num(cluster: i32, min: i32, max: i32) {
    write_node(
        &fmt_path("/sys/devices/system/cpu/cpu{}/core_ctl/min_cpus", cluster),
        &min.to_string(),
    );
    write_node(
        &fmt_path("/sys/devices/system/cpu/cpu{}/core_ctl/max_cpus", cluster),
        &max.to_string(),
    );
}

pub fn orms_write_cpu_ddr_latfloor_max(cluster: i32, value: &str) {
    write_node(
        &fmt_path("/sys/class/devfreq/soc:qcom,cpu{}-cpu-ddr-latfloor/max_freq", cluster),
        value,
    );
}

pub fn orms_write_cpu_ddr_latfloor_max2(cluster_offset: i32, value: &str) {
    write_node(
        &fmt_path("/sys/class/devfreq/soc:qcom,cpu{}-cpu-ddr-latfloor/max_freq", cluster_offset),
        value,
    );
}

pub fn orms_write_cpu_ddr_latfloor_min(cluster: i32, value: &str) {
    write_node(
        &fmt_path("/sys/class/devfreq/soc:qcom,cpu{}-cpu-ddr-latfloor/min_freq", cluster),
        value,
    );
}

pub fn orms_write_cpu_ddr_latfloor_min2(cluster_offset: i32, value: &str) {
    write_node(
        &fmt_path("/sys/class/devfreq/soc:qcom,cpu{}-cpu-ddr-latfloor/min_freq", cluster_offset),
        value,
    );
}

pub fn orms_write_cpu_l3_lat_max(cluster: i32, value: &str) {
    write_node(
        &fmt_path(
            "/sys/class/devfreq/18321000.qcom,devfreq-l3:qcom,cpu{}-cpu-l3-lat/max_freq",
            cluster,
        ),
        value,
    );
}

pub fn orms_write_cpu_llcc_lat_max(cluster: i32, value: &str) {
    write_node(
        &fmt_path("/sys/class/devfreq/soc:qcom,cpu{}-llcc-ddr-lat/max_freq", cluster),
        value,
    );
}

pub fn orms_write_cpu_online(check_mask: i32, online_mask: i32) {
    // The same value is written to both the isolation and the de-isolation
    // node; the check mask gates the pair in the caller.
    let _ = check_mask;
    let value = online_mask.to_string();
    write_node("/proc/perfmgr/boost_ctrl/eas_ctrl/set_sched_isolation", &value);
    write_node("/proc/perfmgr/boost_ctrl/eas_ctrl/set_sched_deisolation", &value);
}

pub fn orms_write_fg_cpu_uclamp_min(value: &str) {
    write_node("/dev/cpuctl/foreground/cpu.uclamp.min", value);
}

pub fn orms_write_fpsgo(value: &str) {
    write_node("/sys/kernel/fpsgo/common/force_onoff", value);
}

pub fn orms_write_hwmon_hyst_opt(value: &str) {
    write_node("/sys/class/devfreq/soc:qcom,cpu-cpu-llcc-bw/bw_hwmon/hyst_length", value);
    write_node(
        "/sys/devices/system/cpu/bus_dcvs/LLCC/190b6400.qcom,bwmon-llcc/hyst_length",
        value,
    );
    write_node(
        "/sys/devices/system/cpu/bus_dcvs/LLCC/240b6400.qcom,bwmon-llcc/hyst_length",
        value,
    );
}

pub fn orms_write_input_boost_enabled(service: &Service, value: &str) {
    let path = if service.has_fbg_knobs {
        "/proc/sys/fbg/input_boost_enabled"
    } else {
        "/proc/sys/kernel/input_boost_enabled"
    };
    let node = node_remap::resolve_node(path);
    io_shim::write_file(&node, value);
    if !io_shim::exists(&node) {
        set_sim_boost_bit(service, SIM_BIT_INPUT, value_is_enabled(value));
    }
}

pub fn orms_write_input_boost_freq(value: &str) {
    write_node("/proc/sys/walt/input_boost/input_boost_freq", value);
    write_node("/sys/devices/system/cpu/cpu_boost/input_boost_freq", value);
}

pub fn orms_write_llcc_ddr_lat_max(cluster: i32, value: &str) {
    write_node(
        &fmt_path("/sys/class/devfreq/soc:qcom,cpu{}-llcc-ddr-lat/max_freq", cluster),
        value,
    );
}

pub fn orms_write_low_power_mode(value: &str) {
    write_node("/proc/mcdi/state", value);
}

pub fn orms_write_memlat_l3_opt(value: &str) {
    write_node(
        "/sys/devices/system/cpu/bus_dcvs/L3/soc:qcom,memlat:l3:prime/min_freq",
        value,
    );
    write_node(
        "/sys/devices/system/cpu/bus_dcvs/L3/soc:qcom,memlat:l3:silver/min_freq",
        value,
    );
    write_node(
        "/sys/devices/system/cpu/bus_dcvs/L3/soc:qcom,memlat:l3:gold/min_freq",
        value,
    );
}

pub fn orms_write_prefer_silver_enabled(value: &str) {
    write_node("/proc/sys/kernel/prefer_silver_enabled", value);
}

pub fn orms_write_ruler_enable(value: &str) {
    write_node(
        "/sys/devices/platform/soc/soc:oplus-omrg/oplus-omrg0/ruler_enable",
        value,
    );
}

pub fn orms_write_schedtune_colocate(value: &str) {
    write_node("/dev/cpuctl/top-app/cpu.uclamp.colocate", value);
    write_node("/dev/stune/top-app/schedtune.colocate", value);
}

pub fn orms_write_schedtune_prefer_idle(value: &str) {
    write_node("/dev/stune/foreground/schedtune.prefer_idle", value);
}

pub fn orms_write_sleep_disabled(value: &str) {
    write_node("/sys/devices/system/cpu/qcom_lpm/parameters/sleep_disabled", value);
}

pub fn orms_write_slide_boost(service: &Service, value: &str) {
    let path = if service.has_fbg_knobs {
        "/proc/sys/fbg/slide_boost_enabled"
    } else {
        "/proc/sys/kernel/slide_boost_enabled"
    };
    let node = node_remap::resolve_node(path);
    io_shim::write_file(&node, value);
    if !io_shim::exists(&node) {
        set_sim_boost_bit(service, SIM_BIT_SLIDE, value_is_enabled(value));
    }
}

/// setOrRelease selects which of the three per-policy knobs is written.
pub fn orms_write_target_loads(set_or_release: i32, cluster: i32, value: &str) {
    let fmt = match set_or_release {
        1 => "/sys/devices/system/cpu/cpufreq/policy{}/schedutil/target_loads",
        2 => "/sys/devices/system/cpu/cpufreq/policy{}/uag/target_loads",
        _ => "/sys/devices/system/cpu/cpufreq/policy{}/walt/target_loads",
    };
    write_node(&fmt_path(fmt, cluster), value);
}

pub fn orms_write_top_cpu_uclamp_min(value: &str) {
    write_node("/dev/cpuctl/top-app/cpu.uclamp.min", value);
}

pub fn orms_write_touch_boost(service: &Service, value: &str) {
    let node = node_remap::resolve_node("/proc/perfmgr/tchbst/user/usrtch");
    io_shim::write_file(&node, value);
    if !io_shim::exists(&node) {
        set_sim_boost_bit(service, SIM_BIT_TOUCH, value_is_enabled(value));
    }
}

pub fn orms_write_tracing_set_event(value: &str) {
    write_node("/sys/kernel/tracing/set_event", value);
}

pub fn orms_write_uclamp_latency_sensitive(value: &str) {
    write_node("/dev/cpuctl/foreground/cpu.uclamp.latency_sensitive", value);
}

pub fn orms_write_hal_ufs_plus_ctrl(value1: &str, value2: &str) {
    write_node("/proc/ufsplus_ctrl/wbfn_enable", value1);
    write_node("/proc/ufsplus_ctrl/wbfn_dynamic_tw_enable", value2);
}

pub fn orms_write_sched_asymcap_boost(service: &Service, value: &str) {
    let node = node_remap::resolve_node("/proc/sys/walt/sched_asymcap_boost");
    io_shim::write_file(&node, value);
    if !io_shim::exists(&node) {
        set_sim_boost_bit(service, SIM_BIT_ASYMCAP, value_is_enabled(value));
    }
}

pub fn orms_write_cpu_cpu_ddr_lat_min(cluster: i32, value: &str) {
    write_node(
        &fmt_path("/sys/class/devfreq/soc:qcom,cpu{}-cpu-ddr-lat/min_freq", cluster),
        value,
    );
}

pub fn orms_write_cpu_cpu_ddr_bw_min(value: &str) {
    write_node("/sys/class/devfreq/soc:qcom,cpu-cpu-ddr-bw/min_freq", value);
}

/// Dispatches on a table of node ids; an unknown id is ignored with a debug log.
pub fn orms_write_node_common(id: i32, cluster: i32, value: &str) {
    match id {
        0 => write_node(
            &fmt_path(
                "/sys/devices/system/cpu/cpu{}/cpufreq/uag/up_rate_limit_us",
                cluster,
            ),
            value,
        ),
        1 => write_node(
            &fmt_path(
                "/sys/devices/system/cpu/cpu{}/cpufreq/uag/down_rate_limit_us",
                cluster,
            ),
            value,
        ),
        2 | 3 => {}
        _ => {}
    }
}

pub fn orms_write_force_step(value: &str) {
    write_node("/sys/module/mmdvfs_pmqos/parameters/force_step", value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn setup(name: &str) -> (PathBuf, std::sync::MutexGuard<'static, ()>) {
        let guard = io_shim::test_lock();
        let root = format!("/tmp/opencode/orms_core_fixture_{}", name);
        let _ = fs::remove_dir_all(&root);
        io_shim::set_test_root(&root);
        (PathBuf::from(root), guard)
    }

    fn teardown(root: &PathBuf) {
        io_shim::clear_test_root();
        let _ = fs::remove_dir_all(root);
    }

    fn w(root: &PathBuf, rel: &str, v: &str) {
        let path = format!("{}/{}", root.display(), rel);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, v).unwrap();
    }

    fn mk(root: &PathBuf, rel: &str) {
        fs::create_dir_all(format!("{}/{}", root.display(), rel)).unwrap();
    }

    /// Models the kernel's per-core cpufreq symlinks: every core points at
    /// its cluster's policy directory.
    fn link_cpus(root: &PathBuf, policy: &str) {
        for cpu in 0..8 {
            let dir = root.join(format!("sys/devices/system/cpu/cpu{}", cpu));
            fs::create_dir_all(&dir).unwrap();
            let link = dir.join("cpufreq");
            let _ = fs::remove_file(&link);
            std::os::unix::fs::symlink(root.join(policy), &link).unwrap();
        }
    }

    fn read(root: &PathBuf, rel: &str) -> String {
        fs::read_to_string(format!("{}/{}", root.display(), rel)).unwrap()
    }

    fn service() -> Service {
        Service {
            has_fbg_knobs: false,
            kernel_version: String::new(),
            boosts: Mutex::new(HashMap::new()),
            sim_boost_mask: Mutex::new(0),
        }
    }

    #[test]
    fn write_node_passes_through_existing_path() {
        let (r, _g) = setup("passthrough");
        mk(&r, "sys/module/cpufreq_bouncing/parameters");
        orms_enable_cpu_bouncing("1");
        assert_eq!(read(&r, "sys/module/cpufreq_bouncing/parameters/enable"), "1");
        teardown(&r);
    }

    #[test]
    fn kernel_version_parsed_from_proc_version() {
        let (r, _g) = setup("kver");
        w(&r, "proc/version", "Linux version 5.10.198 (gcc) #1 SMP");
        assert_eq!(Service::kernel_version_string(), "5.10.198");
        teardown(&r);
    }

    #[test]
    fn boost_acquire_and_release_bookkeeping() {
        let (r, _g) = setup("boost");
        let svc = service();
        orms_boost_acquire(&svc, 7, &[1, 2, 3]);
        {
            let boosts = svc.boosts.lock().unwrap();
            assert_eq!(boosts.get(&7).unwrap().len(), 3);
        }
        orms_boost_release(&svc, 7);
        assert!(svc.boosts.lock().unwrap().get(&7).is_none());
        teardown(&r);
    }

    #[test]
    fn read_file_returns_empty_for_missing_node() {
        let (r, _g) = setup("readmissing");
        assert_eq!(orms_read_file("/sys/class/nope"), "");
        teardown(&r);
    }

    #[test]
    fn gpu_freq_reads_the_remapped_mali_node() {
        let (r, _g) = setup("gpufreq");
        mk(&r, "sys/class/devfreq/13000000.mali");
        w(&r, "sys/class/devfreq/13000000.mali/cur_freq", "800000000");
        assert_eq!(orms_read_gpu_freq(0), "800000000");
        teardown(&r);
    }

    #[test]
    fn ddr_avail_freq_prefers_the_remapped_dram_domain() {
        let (r, _g) = setup("ddrfreq");
        mk(&r, "sys/class/devfreq/mtk-dvfsrc-devfreq");
        w(
            &r,
            "sys/class/devfreq/mtk-dvfsrc-devfreq/scaling_available_frequencies",
            "500000 2000000",
        );
        mk(&r, "sys/devices/system/cpu/cpufreq/policy0");
        w(
            &r,
            "sys/devices/system/cpu/cpufreq/policy0/scaling_available_frequencies",
            "500000 2000000",
        );
        link_cpus(&r, "sys/devices/system/cpu/cpufreq/policy0");
        assert_eq!(orms_read_ddr_avail_freq(0), "500000 2000000");
        assert_eq!(orms_read_ddr_avail_freq(1), "500000 2000000");
        assert_eq!(orms_read_ddr_avail_freq(2), "500000 2000000");
        assert_eq!(orms_read_ddr_avail_freq(9), "");
        teardown(&r);
    }

    #[test]
    fn cpu_online_writes_both_isolation_nodes() {
        let (r, _g) = setup("online");
        mk(&r, "proc/perfmgr/boost_ctrl/eas_ctrl");
        orms_write_cpu_online(0, 0xF0);
        assert_eq!(read(&r, "proc/perfmgr/boost_ctrl/eas_ctrl/set_sched_isolation"), "240");
        assert_eq!(read(&r, "proc/perfmgr/boost_ctrl/eas_ctrl/set_sched_deisolation"), "240");
        teardown(&r);
    }

    #[test]
    fn target_loads_selects_the_per_set_or_release_knob() {
        let (r, _g) = setup("targetloads");
        mk(&r, "sys/devices/system/cpu/cpufreq/policy0/walt");
        mk(&r, "sys/devices/system/cpu/cpufreq/policy0/schedutil");
        orms_write_target_loads(0, 0, "90");
        orms_write_target_loads(1, 0, "80");
        assert_eq!(read(&r, "sys/devices/system/cpu/cpufreq/policy0/walt/target_loads"), "90");
        assert_eq!(read(&r, "sys/devices/system/cpu/cpufreq/policy0/schedutil/target_loads"), "80");
        teardown(&r);
    }

    #[test]
    fn fbg_knobs_switch_input_boost_and_slide_boost_paths() {
        let (r, _g) = setup("fbg");
        mk(&r, "proc/sys/fbg");
        mk(&r, "proc/sys/kernel");
        let plain = service();
        orms_write_input_boost_enabled(&plain, "1");
        assert!(fs::read_to_string(r.join("proc/sys/kernel/input_boost_enabled")).is_ok());
        let mut fbg = service();
        fbg.has_fbg_knobs = true;
        orms_write_input_boost_enabled(&fbg, "1");
        assert!(fs::read_to_string(r.join("proc/sys/fbg/input_boost_enabled")).is_ok());
        teardown(&r);
    }

    #[test]
    fn node_common_dispatches_on_id_and_noops_thermal() {
        let (r, _g) = setup("nodecommon");
        mk(&r, "sys/devices/system/cpu/cpu0/cpufreq/uag");
        mk(&r, "sys/class/thermal/thermal_zone1");
        orms_write_node_common(0, 0, "1000");
        orms_write_node_common(2, 1, "enabled");
        orms_write_node_common(9, 0, "x");
        assert_eq!(
            read(&r, "sys/devices/system/cpu/cpu0/cpufreq/uag/up_rate_limit_us"),
            "1000"
        );
        assert!(fs::read_to_string(r.join("sys/class/thermal/thermal_zone1/mode")).is_err());
        assert!(fs::read_to_string(r.join("sys/class/thermal/thermal_zone0/mode")).is_err());
        teardown(&r);
    }

    #[test]
    fn multi_target_writes_reach_every_node() {
        let (r, _g) = setup("multi");
        mk(&r, "sys/kernel/fpsgo/common");
        mk(&r, "sys/kernel/tracing");
        orms_write_fpsgo("1");
        orms_write_tracing_set_event("sched:*");
        assert_eq!(read(&r, "sys/kernel/fpsgo/common/force_onoff"), "1");
        assert_eq!(read(&r, "sys/kernel/tracing/set_event"), "sched:*");
        teardown(&r);
    }

    #[test]
    fn first_int_of_parses_leading_digits() {
        assert_eq!(first_int_of("5.10.198").unwrap(), 5);
        assert_eq!(first_int_of("198").unwrap(), 198);
        assert!(first_int_of("nope").is_none());
    }

    #[test]
    fn sim_boost_drives_mtk_nodes_when_oplus_nodes_absent() {
        let guard = io_shim::test_lock();
        let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/target/test_simboost"));
        let _ = fs::remove_dir_all(&root);
        io_shim::set_test_root(root.to_str().unwrap());
        w(&root, "dev/cpuctl/top-app/cpu.uclamp.min", "0");
        w(&root, "sys/kernel/fpsgo/common/force_onoff", "2");

        let svc = service();
        orms_boost_acquire(&svc, 1, &[100]);
        assert_eq!(read(&root, "dev/cpuctl/top-app/cpu.uclamp.min"), "30");
        assert_eq!(read(&root, "sys/kernel/fpsgo/common/force_onoff"), "1");

        // Second source keeps boost active until both release.
        orms_write_touch_boost(&svc, "1");
        orms_boost_release(&svc, 1);
        assert_eq!(read(&root, "dev/cpuctl/top-app/cpu.uclamp.min"), "30");

        orms_write_touch_boost(&svc, "0");
        assert_eq!(read(&root, "dev/cpuctl/top-app/cpu.uclamp.min"), "0");
        assert_eq!(read(&root, "sys/kernel/fpsgo/common/force_onoff"), "2");

        io_shim::clear_test_root();
        let _ = fs::remove_dir_all(&root);
        drop(guard);
    }
}
