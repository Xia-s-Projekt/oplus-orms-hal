# Porting notes — OplusOrms

Working log for the C++ → Rust rewrite. The README is the durable description;
this is what actually happened, including the mistakes.

## Source of truth

Two things were recovered from before anything was written:

1. **The shipped binary.** `erofs/vendor/bin/hw/vendor.oplus.hardware.ormsHalService-aidl-service`,
   154840 bytes, linked against libc++ (the `_ZNSt3__1…` symbols). `llvm-strings`
   on it yields the registered fqname, the interface hash and the log tag —
   enough to pin the identity, not the behaviour.
2. **The historical C++ tree** at `../OplusPowerRpmhOrms`. `src/orms/NodeRemap.cpp`
   is the remap spec; the ORMS method bodies live beside it. This is where the
   actual semantics came from.

The AIDL file was reconstructed from the method list and the parcel shapes in
the shipped binary; every method's argument order and types were checked against
the C++ call sites.

## Structure choice

URCC was the template: a host-testable core with the binder layer confined to
the binary. That split is why ORMS could reach 23 passing tests without a device.

- `lib.rs` re-exports `core`, `io_shim`, `node_remap` — all plain Rust, no NDK.
- `bin/orms_service.rs` includes `wire.rs` and `hal.rs` by path, and is the only
  file that calls into `libbinder_ndk`.
- `io_shim::resolve` is the single choke point for absolute paths. Everything in
  `core.rs` goes through `write_node` / `read_node`, which call it.

The point of the shim is that the test root is a process-global, so tests
cannot run in parallel against different fixtures — hence `test_lock()`.

## Bugs found while writing the tests

These are worth recording because each one was a real defect, not a test typo.

### `resolve` was not idempotent

`resolve` prepended the test root to any absolute path. But callers legitimately
hand a resolved path back in: `list_dir` returns resolved entries, symlink
canonicalisation returns a resolved path, and the domain discovery loop reads
back what it just resolved. The second pass produced
`/tmp/…/sys/class/devfreq/mtk-dvfsrc-devfreq` → `/tmp/…/tmp/…/sys/…`, a path that
does not exist, so every read silently returned empty.

Fix: if the path already starts with the root, return it unchanged. The
on-device behaviour is identical (the root is `None`, so the branch is dead
there); only the test path changes.

### Poisoned mutex cascaded

One panicking test poisoned the shared fixture lock, and every subsequent test
then failed with `PoisonError` rather than its own assertion. That masked the
real failures for a full run.

Fix: a small `lock<T>(&'static Mutex<T>)` helper that recovers the guard with
`unwrap_or_else(|e| e.into_inner())`. Correct here specifically because the lock
only serialises tests — it protects no invariant, so a poisoned guard is safe to
reuse. The same fix went into the property store, which has the same
global-static shape and the same risk on a panic during init.

### Fixture helpers wrote into directories they never created

`w(root, rel, v)` did `fs::write` on the joined path. In the real filesystem the
parent always exists; in a fresh fixture it does not, so the helper panicked with
`NotFound` before the code under test ever ran. Fixed by `create_dir_all` on the
parent inside `w`.

### Two assertions compared logical paths to resolved paths

`resolve_node("/sys/class/devfreq/soc:qcom,cpu4-llcc-ddr-lat/max_freq")` returns
the *resolved* path, which under test is `/tmp/opencode/orms_fixture_*/sys/…`.
The assertions compared against the bare logical path. On device the two are
equal; in the fixture they never are. Fixed by wrapping the expected value in
`io_shim::resolve` too.

### `ormsReadDdrAvailFreq` argument is a cluster index, not a CPU

The test called `orms_read_ddr_avail_freq(4)` expecting cluster-1 data. The
shipped binary indexes `kPaths[3]` with the argument directly and returns empty
outside `0..2`, so `4` correctly produced `""` and the test was wrong. This is
the kind of thing that has to be read off the shipped code, not guessed — a
normal-looking CPU number is a plausible-looking bug.

### `llcc-lat` is not the GPU

A test asserted that `soc:qcom,cpu0-cpu-llcc-lat/min_freq` maps to the Mali
devfreq node. It does not: `llcc-lat` and `l3-lat` are per-cluster CPU latency
domains and map to the cpufreq policy; only `kgsl`/`mali` map to the GPU. The
original `NodeRemap.cpp` is explicit about this, and the remap was right.

## Missing cpufreq symlinks

`scan_cpufreq` originally walked `cpu0..N` and read `cpuN/cpufreq`, giving up on
a core whose symlink was absent. Real kernels sometimes drop the per-core
symlinks (or the fixture was simply not modelling them). Added `policy_dirs()`:
symlink discovery first, and if that yields nothing, enumerate
`/sys/devices/system/cpu/cpufreq/policy*` directly and sort. Same for the
per-core lookup — `cpu_policy_for_core` falls back to
`cpu_policy_by_index(cluster_index(cpu))` with the MT6893 mapping
`cpu0..3 → 0`, `cpu4..6 → 1`, `cpu7 → 2`.

## Binary identity

The first cross-build assembled the instance name with
`format!("{}/{}", DESCRIPTOR, "default")`, which means the literal fqname never
appears contiguously in the binary — `llvm-strings` finds the descriptor and
`/"default"` separately, differing from the shipped C++ build where the whole
string is one constant. Not a functional problem (the registered name is
identical at runtime) but it defeats offline verification of the artifact, which
is the whole point of doing this without flashing.

Fixed with a `const DEFAULT_INSTANCE` literal, used on the default path. The
`OPLUS_ORMS_INSTANCE` environment override still builds its name dynamically,
which is correct — that path is not the shipped one.

Verified after the change:

```
vendor.oplus.hardware.ormsHalService.IOrmsAidlHalService/default   1
45b9fba87d14d35c461848d6bb6cc13324f947c3                           1
ORMS_AIDL_HAL                                                      1
```

## Build characteristics

- `DT_NEEDED`: `libdl.so liblog.so libc.so libbinder_ndk.so`. No libc++.
- The `__cxa_atexit` symbol that trips a naive C++-leak grep resolves from
  libc, not from a C++ runtime.
- Two consecutive clean builds: identical sha256
  (`79320df90dc264572f3378243ca92475531374edc2881c7290795797c7a4d860`).
- `libbinder_ndk.so` is linked from the ROM tree rather than the NDK stub —
  `AServiceManager_addService` and the `ABinderProcess_*` entry points are not
  in the public stub.
- Pinned toolchain 1.99.1; the reproducibility above holds only at that version.

## Not verified

- No device run. The service has not been started from this binary; the
  liveness check waits on the next vendor repack, which is deliberately out of
  scope for this repo.
- Rebuild-vs-shipped binary comparison is impossible by construction (different
  compiler), so verification is interface/hash/name/DT_NEEDED only.