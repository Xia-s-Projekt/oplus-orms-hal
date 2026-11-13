// NodeRemap — device path resolution for the Orms HAL.
//
// The shipped ORMS HAL was written for a Qualcomm platform: most of the knobs
// it drives live under soc:qcom / qcom,bwmon-* / walt / kgsl paths that do not
// exist on the MT6893 (Transsion) kernel this ROM runs on. The ORIGINAL path
// is always tried first so the port behaves identically on a Qualcomm kernel;
// only when the original node is absent do we fall back to the MTK/Transsion
// node, and only where that node drives the same control. A node with no
// counterpart is deliberately left unmapped rather than redirected at an
// unrelated knob.

use crate::io_shim;

/// Only nodes whose MTK counterpart drives the same control are listed:
/// kgsl -> mali is the same GPU frequency on a different GPU driver. The
/// cpufreq governor nodes resolve at run time (see resolve_governor_node) and
/// the devfreq families resolve against the domains this kernel actually has
/// (see resolve_devfreq_node).
const KGSL_TO_MALI: [(&str, &str); 1] = [(
    "/sys/class/kgsl/kgsl-3d0/devfreq/cur_freq",
    "/sys/class/devfreq/13000000.mali/cur_freq",
)];

/// Highest CPU index worth probing for a %d-formatted path.
const MAX_CPU_INDEX: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainKind {
    Gpu,
    Dram,
    Other,
}

#[derive(Debug, Clone)]
pub struct FreqDomain {
    pub name: String,
    pub path: String,
    pub kind: DomainKind,
    pub available_frequencies_path: String,
    pub cur_freq_path: String,
    pub min_freq_path: String,
    pub max_freq_path: String,
    pub available_frequencies: String,
    pub cur_freq: String,
    pub min_freq: String,
    pub max_freq: String,
    pub min_max_writable: bool,
}

/// The devfreq `type` attribute is empty on the MediaTek kernel, so the
/// domain class comes from the name the MediaTek drivers register: the DRAM
/// controller as "dvfsrc" and the GPU as "mali".
fn classify(lower_name: &str) -> DomainKind {
    if lower_name.contains("mali")
        || lower_name.contains("kgsl")
        || lower_name.contains("gpu")
    {
        return DomainKind::Gpu;
    }
    if lower_name.contains("ddr")
        || lower_name.contains("dram")
        || lower_name.contains("lpddr")
        || lower_name.contains("mem")
        || lower_name.contains("dvfsrc")
        || lower_name.contains("drvrc")
    {
        return DomainKind::Dram;
    }
    DomainKind::Other
}

/// The attribute that holds `wanted` in `dir`, preferring the plain name and
/// falling back to the scaling_ prefixed one the 5.10 cpufreq uses.
fn attribute_path(dir: &str, wanted: &str, scaling: &str) -> String {
    let plain = format!("{}/{}", dir, wanted);
    if io_shim::exists(&plain) {
        return plain;
    }
    let scaled = format!("{}/{}", dir, scaling);
    if io_shim::exists(&scaled) {
        return scaled;
    }
    String::new()
}

fn read_node_content(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    io_shim::read_file(path).unwrap_or_default()
}

fn scan_devfreq() -> Vec<FreqDomain> {
    let root = io_shim::resolve(std::path::Path::new("/sys/class/devfreq"));
    let mut out = Vec::new();
    for name in io_shim::list_dir(root.to_str().unwrap_or("")) {
        let path = {
            let mut p = root.clone();
            p.push(&name);
            p.to_string_lossy().into_owned()
        };
        if !io_shim::is_dir(&path) {
            continue;
        }
        let kind = classify(&name.to_lowercase());
        let available_frequencies_path =
            attribute_path(&path, "available_frequencies", "scaling_available_frequencies");
        let cur_freq_path = attribute_path(&path, "cur_freq", "scaling_cur_freq");
        let min_freq_path = attribute_path(&path, "min_freq", "scaling_min_freq");
        let max_freq_path = attribute_path(&path, "max_freq", "scaling_max_freq");
        out.push(FreqDomain {
            available_frequencies: read_node_content(&available_frequencies_path),
            cur_freq: read_node_content(&cur_freq_path),
            min_freq: read_node_content(&min_freq_path),
            max_freq: read_node_content(&max_freq_path),
            min_max_writable: io_shim::writable(&min_freq_path)
                && io_shim::writable(&max_freq_path),
            name,
            path,
            kind,
            available_frequencies_path,
            cur_freq_path,
            min_freq_path,
            max_freq_path,
        });
    }
    out
}

/// The per-cpu cpufreq symlink is the authoritative mapping from a core to
/// its cluster (this kernel: cpu0..3 -> policy0, cpu4..6 -> policy4,
/// cpu7 -> policy7). Resolved by following the symlink rather than reading
/// it, because readlink is what makes the kernel's own mapping visible.
fn policy_of_cpu(cpu: i32) -> Option<String> {
    let link = io_shim::resolve(std::path::Path::new(&format!(
        "/sys/devices/system/cpu/cpu{}/cpufreq",
        cpu
    )));
    io_shim::read_link_root(&link)
}

/// Every cpufreq policy directory, resolved through the per-core symlinks
/// first. Kernels that do not publish those symlinks are handled by
/// enumerating /sys/devices/system/cpu/cpufreq/policy* directly.
fn policy_dirs() -> Vec<String> {
    let mut policies = Vec::new();
    for cpu in 0..MAX_CPU_INDEX {
        if let Some(p) = policy_of_cpu(cpu) {
            if !policies.contains(&p) {
                policies.push(p);
            }
        }
    }
    if policies.is_empty() {
        let base = io_shim::resolve(std::path::Path::new("/sys/devices/system/cpu/cpufreq"));
        for name in io_shim::list_dir(&base) {
            if !name.starts_with("policy") {
                continue;
            }
            let mut path = base.clone();
            path.push(&name);
            if io_shim::is_dir(&path) {
                policies.push(path.to_string_lossy().into_owned());
            }
        }
        policies.sort();
    }
    policies
}

fn scan_cpufreq() -> Vec<FreqDomain> {
    let mut out = Vec::new();
    for policy in policy_dirs() {
        let available_frequencies_path =
            attribute_path(&policy, "available_frequencies", "scaling_available_frequencies");
        let cur_freq_path = attribute_path(&policy, "cur_freq", "scaling_cur_freq");
        let min_freq_path = attribute_path(&policy, "min_freq", "scaling_min_freq");
        let max_freq_path = attribute_path(&policy, "max_freq", "scaling_max_freq");
        out.push(FreqDomain {
            available_frequencies: read_node_content(&available_frequencies_path),
            cur_freq: read_node_content(&cur_freq_path),
            min_freq: read_node_content(&min_freq_path),
            max_freq: read_node_content(&max_freq_path),
            min_max_writable: io_shim::writable(&min_freq_path)
                && io_shim::writable(&max_freq_path),
            name: policy
                .rsplit('/')
                .next()
                .unwrap_or(&policy)
                .to_string(),
            path: policy.clone(),
            kind: DomainKind::Other,
            available_frequencies_path,
            cur_freq_path,
            min_freq_path,
            max_freq_path,
        });
    }
    out
}

fn discovered_domains() -> Vec<FreqDomain> {
    let mut domains = scan_devfreq();
    domains.extend(scan_cpufreq());
    domains
}

pub fn dram_domain() -> Option<FreqDomain> {
    discovered_domains()
        .into_iter()
        .find(|d| d.kind == DomainKind::Dram)
}

pub fn gpu_domain() -> Option<FreqDomain> {
    discovered_domains()
        .into_iter()
        .find(|d| d.kind == DomainKind::Gpu)
}

/// MT6893 cluster layout used only when the per-core symlink is missing:
/// cpu0..3 -> policy0, cpu4..6 -> policy1, cpu7 -> policy2.
fn cluster_index(cpu: i32) -> usize {
    match cpu {
        0..=3 => 0,
        4..=6 => 1,
        _ => 2,
    }
}

pub fn cpu_policy_for_core(cpu: i32) -> Option<FreqDomain> {
    match policy_of_cpu(cpu) {
        Some(policy) => discovered_domains().into_iter().find(|d| d.path == policy),
        None => cpu_policy_by_index(cluster_index(cpu)),
    }
}

pub fn cpu_policy_by_index(index: usize) -> Option<FreqDomain> {
    discovered_domains()
        .into_iter()
        .filter(|d| d.path.contains("/cpufreq/policy"))
        .nth(index)
}

/// The available frequencies of the named cpu's cluster, scaled from kHz to
/// Hz so the unit of the original DDR node is kept. `source` reports which
/// node actually answered, for diagnostics.
pub fn ddr_available_frequencies(cpu: i32) -> (String, String) {
    if let Some(policy) = cpu_policy_for_core(cpu) {
        if !policy.available_frequencies.is_empty() {
            let scaled = policy
                .available_frequencies
                .split_whitespace()
                .map(|khz| format!("{}", khz.parse::<i64>().unwrap_or(0) * 1000))
                .collect::<Vec<_>>()
                .join(" ");
            return (scaled, policy.available_frequencies_path);
        }
    }
    (String::new(), String::new())
}

/// The cpu number a Qualcomm devfreq domain name carries: 4 for
/// "soc:qcom,cpu4-llcc-ddr-lat", -1 when the name is not cluster specific
/// ("soc:qcom,cpu-cpu-ddr-bw").
fn cluster_of(name: &str) -> i32 {
    let bytes = name.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if &bytes[i..i + 3] != b"cpu" {
            i += 1;
            continue;
        }
        let mut j = i + 3;
        if j >= bytes.len() || !bytes[j].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut value: i32 = 0;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            value = value * 10 + (bytes[j] - b'0') as i32;
            j += 1;
        }
        return value;
    }
    -1
}

/// The eight Qualcomm devfreq families the shipped HAL drives, resolved
/// against the domains this kernel has by the control they represent, never
/// by name:
///   llcc-ddr-lat, cpu-ddr-lat, cpu-ddr-latfloor, cpu-ddr-bw
///       the memory bandwidth governor        -> the DRAM domain
///   cpu-llcc-lat, cpu-l3-lat
///       the per-cluster L3 latency governor  -> that cluster's cpufreq policy
///   kgsl-3d0
///       the GPU core governor                -> the GPU domain
/// The UFS domain, the GPU bus monitor (busmon) and the bandwidth hwmon have
/// no counterpart and stay unmapped. A frequency written into a mapped node
/// is not rescaled: the kernel clamps it to the domain's own table.
fn resolve_devfreq_node(path: &str) -> String {
    let lower = path.to_lowercase();
    let slash = match lower.rfind('/') {
        Some(s) => s,
        None => return String::new(),
    };
    let attr = &lower[slash + 1..];

    let domain = if let Some(root) = lower.find("/sys/class/devfreq/") {
        &lower[root + 17..slash]
    } else if lower.contains("/sys/class/kgsl/") {
        "kgsl-3d0"
    } else {
        return String::new();
    };
    if domain.is_empty() {
        return String::new();
    }

    let cluster = cluster_of(domain);
    let is_gpu = domain.contains("kgsl") || domain.contains("mali");

    if attr == "available_frequencies" {
        if is_gpu {
            return gpu_domain()
                .map(|d| d.available_frequencies_path)
                .unwrap_or_default();
        }
        if domain.contains("ddr") || domain.contains("llcc") || domain.contains("lat") {
            return dram_domain()
                .map(|d| d.available_frequencies_path)
                .unwrap_or_default();
        }
        return String::new();
    }

    if attr != "min_freq" && attr != "max_freq" {
        return String::new();
    }
    if domain.contains("busmon") {
        return String::new();
    }
    if is_gpu {
        let gpu = match gpu_domain() {
            Some(g) => g,
            None => return String::new(),
        };
        return if attr == "min_freq" {
            gpu.min_freq_path
        } else {
            gpu.max_freq_path
        };
    }
    if domain.contains("l3-lat") || domain.contains("llcc-lat") {
        let policy = if cluster >= 0 {
            cpu_policy_for_core(cluster)
        } else {
            cpu_policy_by_index(0)
        };
        let policy = match policy {
            Some(p) => p,
            None => return String::new(),
        };
        return if attr == "min_freq" {
            policy.min_freq_path
        } else {
            policy.max_freq_path
        };
    }
    if domain.contains("ddr") {
        let dram = match dram_domain() {
            Some(d) => d,
            None => return String::new(),
        };
        return if attr == "min_freq" {
            dram.min_freq_path
        } else {
            dram.max_freq_path
        };
    }
    String::new()
}

/// The governor attributes the shipped HAL addresses, with the attribute
/// names the active governor may use for the same control, most specific
/// first. up/down_rate_limit_us are the directional ramp-rate knobs;
/// schedutil and schedhorizon expose a single undirectional rate_limit_us,
/// the closest equivalent of the same control.
const GOVERNOR_ATTRS: [(&str, [&str; 3]); 4] = [
    ("up_rate_limit_us", ["up_rate_limit_us", "rate_limit_us", ""]),
    ("down_rate_limit_us", ["down_rate_limit_us", "rate_limit_us", ""]),
    ("target_loads", ["target_loads", "", ""]),
    ("above_hispeed_delay", ["above_hispeed_delay", "", ""]),
];

/// Reads the active governor name of the policy directory `policy_dir`.
fn read_active_governor(policy_dir: &str) -> String {
    io_shim::read_file(format!("{}/scaling_governor", policy_dir))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// A governor name comes from a kernel sysfs file but is used to build a
/// path, so it must be a plain identifier before it is used as one.
fn is_safe_governor_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 32 {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// True when `dir` is a cpufreq policy directory: either the per-core
/// symlink (.../cpu<N>/cpufreq) or the policy itself (.../cpufreq/policy<N>).
fn is_policy_dir(dir: &str) -> bool {
    dir.ends_with("/cpufreq") || dir.contains("/cpufreq/")
}

/// Rewrites a "<policy>/<governor>/<attribute>" path to the active
/// governor's directory, when that governor has the attribute. The governor
/// name the kernel publishes is used AS the directory, so the resolution
/// works for every governor this kernel can have, not only the three the
/// shipped binary hardcoded.
fn resolve_governor_node(path: &str) -> String {
    let attr_pos = match path.rfind('/') {
        Some(p) => p,
        None => return String::new(),
    };
    let attr = &path[attr_pos + 1..];

    let candidates: [&str; 4] = GOVERNOR_ATTRS
        .iter()
        .find(|(shipped, _)| shipped == &attr)
        .map(|(_, c)| [c[0], c[1], c[2], ""])
        .unwrap_or(["", "", "", ""]);
    if candidates[0].is_empty() {
        return String::new();
    }

    // The governor directory sits between the policy and the attribute.
    let head = &path[..attr_pos];
    let gov_pos = match head.rfind('/') {
        Some(p) => p,
        None => return String::new(),
    };
    let policy_dir = &head[..gov_pos];
    if !is_policy_dir(policy_dir) {
        return String::new();
    }

    let governor = read_active_governor(policy_dir);
    if !is_safe_governor_name(&governor) {
        return String::new();
    }
    for candidate in candidates.iter() {
        if candidate.is_empty() {
            continue;
        }
        let candidate_path = format!("{}/{}/{}", policy_dir, governor, candidate);
        if io_shim::exists(&candidate_path) {
            return candidate_path;
        }
    }
    String::new()
}

/// Returns the path to actually use for `path`: the original path when it
/// exists, otherwise the MTK/Transsion equivalent when one is known and
/// present, otherwise `path` unchanged (so the original error path runs).
pub fn resolve_node(path: &str) -> String {
    if io_shim::exists(path) {
        return path.to_string();
    }

    let governor_node = resolve_governor_node(path);
    if !governor_node.is_empty() {
        return governor_node;
    }

    for (original, mtk) in KGSL_TO_MALI.iter() {
        if path == *original && io_shim::exists(mtk) {
            return mtk.to_string();
        }
    }

    let discovered = resolve_devfreq_node(path);
    if !discovered.is_empty() {
        return discovered;
    }

    path.to_string()
}

pub fn describe_discovered_domains() -> String {
    let domains = discovered_domains();
    if domains.is_empty() {
        return "no devfreq or cpufreq domains discovered".to_string();
    }
    domains
        .iter()
        .map(|d| {
            format!(
                "{}({:?}) min={} max={} cur={}",
                d.name, d.kind, d.min_freq, d.max_freq, d.cur_freq
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn setup(name: &str) -> (PathBuf, std::sync::MutexGuard<'static, ()>) {
        let guard = io_shim::test_lock();
        let root = format!("/tmp/opencode/orms_fixture_{}", name);
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

    #[test]
    fn original_path_wins_when_present() {
        let (r, _g) = setup("original");
        mk(&r, "sys/class/kgsl/kgsl-3d0/devfreq");
        w(&r, "sys/class/kgsl/kgsl-3d0/devfreq/cur_freq", "500000000");
        assert_eq!(resolve_node("/sys/class/kgsl/kgsl-3d0/devfreq/cur_freq"),
                   "/sys/class/kgsl/kgsl-3d0/devfreq/cur_freq");
        teardown(&r);
    }

    #[test]
    fn kgsl_falls_back_to_mali_when_present() {
        let (r, _g) = setup("kgsl");
        mk(&r, "sys/class/devfreq/13000000.mali");
        w(&r, "sys/class/devfreq/13000000.mali/cur_freq", "500000000");
        assert_eq!(
            resolve_node("/sys/class/kgsl/kgsl-3d0/devfreq/cur_freq"),
            "/sys/class/devfreq/13000000.mali/cur_freq"
        );
        teardown(&r);
    }

    #[test]
    fn absent_node_stays_unmapped() {
        let (r, _g) = setup("absent");
        assert_eq!(
            resolve_node("/sys/class/devfreq/soc:qcom,cpu0-llcc-ddr-lat/max_freq"),
            "/sys/class/devfreq/soc:qcom,cpu0-llcc-ddr-lat/max_freq"
        );
        teardown(&r);
    }

    #[test]
    fn devfreq_family_maps_to_discovered_domains() {
        let (r, _g) = setup("devfreq");
        mk(&r, "sys/class/devfreq/mtk-dvfsrc-devfreq");
        w(&r, "sys/class/devfreq/mtk-dvfsrc-devfreq/max_freq", "4264000");
        assert_eq!(
            resolve_node("/sys/class/devfreq/soc:qcom,cpu4-llcc-ddr-lat/max_freq"),
            io_shim::resolve(std::path::Path::new(
                "/sys/class/devfreq/mtk-dvfsrc-devfreq/max_freq"
            ))
            .to_string_lossy()
        );
        teardown(&r);
    }

    #[test]
    fn gpu_family_maps_to_mali_domain() {
        let (r, _g) = setup("gpu");
        mk(&r, "sys/class/devfreq/13000000.mali");
        w(&r, "sys/class/devfreq/13000000.mali/min_freq", "350000000");
        // The GPU core governor is the kgsl family; the llcc/l3-lat families
        // are per-cluster CPU latency and map to the cpufreq policy instead.
        assert_eq!(
            resolve_node("/sys/class/kgsl/kgsl-3d0/devfreq/min_freq"),
            io_shim::resolve(std::path::Path::new(
                "/sys/class/devfreq/13000000.mali/min_freq"
            ))
            .to_string_lossy()
        );
        teardown(&r);
    }

    #[test]
    fn llcc_lat_maps_to_the_cluster_cpufreq_policy() {
        let (r, _g) = setup("llcc_lat");
        mk(&r, "sys/devices/system/cpu/cpufreq/policy0");
        w(&r, "sys/devices/system/cpu/cpufreq/policy0/scaling_min_freq", "300000");
        w(&r, "sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq", "2000000");
        link_cpus(&r, "sys/devices/system/cpu/cpufreq/policy0");
        assert_eq!(
            resolve_node("/sys/class/devfreq/soc:qcom,cpu0-cpu-llcc-lat/min_freq"),
            io_shim::resolve(std::path::Path::new(
                "/sys/devices/system/cpu/cpufreq/policy0/scaling_min_freq"
            ))
            .to_string_lossy()
        );
        teardown(&r);
    }

    #[test]
    fn busmon_family_stays_unmapped() {
        let (r, _g) = setup("busmon");
        mk(&r, "sys/class/devfreq/13000000.mali");
        assert_eq!(
            resolve_node("/sys/class/devfreq/kgsl-busmon/min_freq"),
            "/sys/class/devfreq/kgsl-busmon/min_freq"
        );
        teardown(&r);
    }

    #[test]
    fn governor_node_uses_active_governor_directory() {
        let (r, _g) = setup("governor");
        mk(&r, "sys/devices/system/cpu/cpufreq/policy0/sugov_ext");
        w(&r, "sys/devices/system/cpu/cpufreq/policy0/scaling_governor", "sugov_ext");
        w(&r, "sys/devices/system/cpu/cpufreq/policy0/sugov_ext/rate_limit_us", "1000");
        // The shipped HAL's "uag" directory does not exist; the active
        // governor's equivalent resolves instead.
        assert_eq!(
            resolve_node("/sys/devices/system/cpu/cpufreq/policy0/uag/up_rate_limit_us"),
            "/sys/devices/system/cpu/cpufreq/policy0/sugov_ext/rate_limit_us"
        );
        teardown(&r);
    }

    #[test]
    fn unsafe_governor_names_are_not_used_as_paths() {
        let (r, _g) = setup("unsafe_gov");
        mk(&r, "sys/devices/system/cpu/cpufreq/policy0/..");
        w(
            &r,
            "sys/devices/system/cpu/cpufreq/policy0/scaling_governor",
            "../../etc",
        );
        assert_eq!(
            resolve_node("/sys/devices/system/cpu/cpufreq/policy0/uag/up_rate_limit_us"),
            "/sys/devices/system/cpu/cpufreq/policy0/uag/up_rate_limit_us"
        );
        teardown(&r);
    }

    #[test]
    fn cluster_of_parses_cpu_number() {
        assert_eq!(cluster_of("soc:qcom,cpu4-llcc-ddr-lat"), 4);
        assert_eq!(cluster_of("soc:qcom,cpu-cpu-ddr-bw"), -1);
    }

    #[test]
    fn ddr_available_freq_scaled_from_khz() {
        let (r, _g) = setup("ddr_avail");
        mk(&r, "sys/devices/system/cpu/cpufreq/policy0");
        w(
            &r,
            "sys/devices/system/cpu/cpufreq/policy0/scaling_available_frequencies",
            "500000 2000000",
        );
        link_cpus(&r, "sys/devices/system/cpu/cpufreq/policy0");
        let (freqs, _source) = ddr_available_frequencies(0);
        assert_eq!(freqs, "500000000 2000000000");
        teardown(&r);
    }
}
