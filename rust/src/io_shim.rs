// File and property plumbing for the Orms HAL.
//
// sysfs reads/writes/stat all go through `resolve`, which prepends the test
// root when set, so the node remap and the domain discovery are exercisable
// against one fake sysfs tree on the host. Properties mirror bionic's
// __system_property_get on Android and an in-memory store off target.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static TEST_ROOT: Mutex<Option<String>> = Mutex::new(None);

/// Prepends the test root when set. Idempotent: a path already under the
/// root is returned unchanged, because callers legitimately hand a resolved
/// path (from a directory listing or a canonicalized symlink) back to the
/// read/write helpers.
pub fn resolve(path: &Path) -> PathBuf {
    if path.is_absolute() {
        let root = lock(&TEST_ROOT).clone();
        if let Some(root) = root {
            if path.starts_with(&root) {
                return path.to_path_buf();
            }
            let stripped = path
                .strip_prefix("/")
                .expect("absolute path strips cleanly");
            return Path::new(&root).join(stripped);
        }
    }
    path.to_path_buf()
}

pub fn read_file<P: AsRef<Path>>(path: P) -> Option<String> {
    let out = fs::read_to_string(resolve(path.as_ref())).ok()?;
    Some(out.trim_end_matches(['\n', '\r']).to_string())
}

pub fn write_file<P: AsRef<Path>>(path: P, value: &str) -> bool {
    match fs::write(resolve(path.as_ref()), value) {
        Ok(()) => true,
        Err(_) => false,
    }
}

pub fn exists<P: AsRef<Path>>(path: P) -> bool {
    fs::metadata(resolve(path.as_ref())).is_ok()
}

pub fn is_dir<P: AsRef<Path>>(path: P) -> bool {
    fs::metadata(resolve(path.as_ref()))
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

/// Follows a cpufreq symlink to the policy directory it points at.
/// /sys/devices/system/cpu/cpu<N>/cpufreq is a symlink the kernel creates
/// at that core's policy, so resolving it is the authoritative core->cluster
/// mapping.
pub fn read_link_root<P: AsRef<Path>>(link: P) -> Option<String> {
    fs::canonicalize(resolve(link.as_ref()))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(unix)]
pub fn writable<P: AsRef<Path>>(path: P) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match fs::metadata(resolve(path.as_ref())) {
        Ok(m) => m.permissions().mode() & 0o200 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
pub fn writable<P: AsRef<Path>>(path: P) -> bool {
    fs::metadata(resolve(path.as_ref())).is_ok()
}

pub fn list_dir<P: AsRef<Path>>(path: P) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(resolve(path.as_ref())) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name != "." && name != ".." {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
pub fn set_test_root(root: &str) {
    *lock(&TEST_ROOT) = Some(root.to_string());
}

#[cfg(test)]
pub fn clear_test_root() {
    *lock(&TEST_ROOT) = None;
}

#[cfg(test)]
static TESTS: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    lock(&TESTS)
}

/// A panicking test poisons its guard; the fixture lock only serializes
/// tests, so recovering the guard is correct here.
fn lock<T>(m: &'static Mutex<T>) -> std::sync::MutexGuard<'static, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(target_os = "android")]
pub fn property_get(key: &str) -> Option<String> {
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> i32;
    }

    const PROP_VALUE_MAX: usize = 92;

    let name = CString::new(key).ok()?;
    let mut buf = vec![0u8; PROP_VALUE_MAX];
    // SAFETY: name is a valid NUL-terminated C string and buf is
    // PROP_VALUE_MAX bytes as bionic's contract requires.
    let n = unsafe { __system_property_get(name.as_ptr(), buf.as_mut_ptr() as *mut c_char) };
    if n <= 0 {
        return None;
    }
    let value = CStr::from_bytes_with_nul(&buf[..=n as usize]).ok()?;
    Some(value.to_string_lossy().into_owned())
}

#[cfg(not(target_os = "android"))]
pub fn property_get(key: &str) -> Option<String> {
    with_props(|p| p.get(key).cloned())
}

#[cfg(not(target_os = "android"))]
fn with_props<R>(f: impl FnOnce(&mut std::collections::HashMap<String, String>) -> R) -> R {
    use std::sync::Mutex;
    static PROPS: Mutex<Option<std::collections::HashMap<String, String>>> = Mutex::new(None);
    let mut store = PROPS.lock().unwrap_or_else(|e| e.into_inner());
    if store.is_none() {
        *store = Some(std::collections::HashMap::new());
    }
    f(store.as_mut().unwrap())
}

#[cfg(not(target_os = "android"))]
pub fn property_set(key: &str, value: &str) -> bool {
    with_props(|p| {
        p.insert(key.to_string(), value.to_string());
        true
    })
}
