# OplusOrms

Rust rewrite of the OPlus Orms HAL (`vendor.oplus.hardware.ormsHalService`) for
the X6739 / MT6893. No C/C++ source, no prebuilt C libraries: the service is a
single Rust binary speaking the binder NDK C API directly and emitting the same
AIDL wire format the shipped backend does.

The historical C++ ORMS/RPMH/powermonitor tree lives untouched in
`../OplusPowerRpmhOrms` and serves as the behavioural spec.

## What the service is

A 46-method AIDL interface over a set of scheduler / DVFS sysfs knobs. Every
method is one of three shapes:

1. **write** a string into a kernel node (`ormsWriteNode`, boost knobs, thermal mode),
2. **read** a node back (`ormsReadDdrAvailFreq`, governor, kernel version),
3. **remap** a Qualcomm-vendor node name onto the MediaTek node that actually
   exists on this SoC, then do 1 or 2.

Point 3 is the entire reason this HAL needed porting rather than copying. The
backend was written for SM8xxx; MT6893 publishes different devfreq class names,
a different GPU governor path and different DVFS domains. `src/node_remap.rs`
carries those translations.

## Layout

```
aidl/vendor/oplus/hardware/ormsHalService/IOrmsAidlHalService.aidl
rust/src/lib.rs          crate root (host-testable halves re-exported)
rust/src/wire.rs         AIDL parcel read/write, status headers
rust/src/hal.rs          AIBinder_Class_define + onTransact dispatch (46 methods)
rust/src/core.rs         the method bodies: node writers and readers
rust/src/node_remap.rs   vendor node discovery and Qualcomm -> MediaTek remap
rust/src/io_shim.rs      absolute-path I/O with a test-root override
rust/src/bin/orms_service.rs  main(): thread pool, addService, joinThreadPool
scripts/build_rust.sh    host tests + aarch64 cross-build + artifact verify
scripts/stage_vendor.sh  copy artifact into the staging tree (no repack)
```

Interface hash, embedded in the binary and checked against the shipped one:

```
45b9fba87d14d35c461848d6bb6cc13324f947c3
```

Service name registered: `vendor.oplus.hardware.ormsHalService.IOrmsAidlHalService/default`
(matches `erofs/vendor/etc/init/ormsHalService-default.rc` and the VINTF
fragment `ormsHalService-default.xml`).

## Build

Pinned toolchain: `rust-toolchain.toml` selects rustc 1.99.1 with the
`aarch64-linux-android` target. The cross-build only reproduces the staged
artifact byte-for-byte at that version.

```sh
bash scripts/build_rust.sh
```

That runs:

1. `cargo test --offline` — 23 host tests over the remap and the node writers,
2. `cargo build --release --offline --target aarch64-linux-android --bins`,
3. renames `orms_service` to its on-device name and prints `NEEDED` + `.comment`.

The `--target` flag is mandatory: `.cargo/config.toml` wires the NDK linker for
that triple only.

Resulting DT_NEEDED set, verified with `llvm-readelf`:

```
libdl.so  liblog.so  libc.so  libbinder_ndk.so
```

No `libc++.so`, no `libstdc++`. The only libc++-looking symbol is
`__cxa_atexit`, which resolves from libc.

`libbinder_ndk.so` is linked from the ROM tree
(`erofs/system/system/lib64/libbinder_ndk.so`), not the NDK stub — the service
needs `AServiceManager_addService` and the `ABinderProcess_*` entry points the
public stub does not export.

Reproducibility: two consecutive clean builds produce identical sha256
(`79320df9…`). Not bit-identical to the shipped C++ binary — different
compiler, different code — but identical in interface, wire format and
registered name, which is what the framework checks.

## The remap, in detail

Discovery runs once, lazily, over the real `/sys/class/devfreq` listing. For
each entry it reads `min_freq` / `max_freq` / `cur_freq` /
`available_frequencies` and classifies the domain:

| class    | matched on                                        |
|----------|---------------------------------------------------|
| `Gpu`    | `mali`, `kgsl` (never `busmon` — that is the bus governor) |
| `Dram`   | `dvfsrc`, `ddr` in the devfreq name                |
| `Other`  | everything else                                    |

cpufreq policies come from the per-core symlinks under
`/sys/devices/system/cpu/cpu*/cpufreq`; when a kernel does not publish them,
the policy directories are enumerated directly. MT6893 clusters are
`cpu0..3 → policy0`, `cpu4..6 → policy1`, `cpu7 → policy2`.

Translation rules (from `OplusPowerRpmhOrms/src/orms/NodeRemap.cpp`):

- an original path that exists on this kernel always wins — the remap is a
  fallback, never an override;
- `kgsl` / `mali` → the discovered GPU domain;
- `ddr` → the discovered memory domain;
- `l3-lat` / `llcc-lat` → the cpufreq policy of the requesting cluster;
- `busmon` → deliberately unmapped;
- governor names are treated as data, never as path fragments — a node whose
  `governor` attribute contains `../` cannot escape the resolved directory.

`available_frequencies` for a memory domain is scaled kHz → Hz, because the
Qualcomm node reported Hz and callers assume those units.

## Semantics worth knowing

- `ormsReadDdrAvailFreq(int)` takes a **cluster index 0..2**, not a CPU
  number. The shipped binary indexes `kPaths[3]` directly; anything outside
  0..2 returns the empty string.
- `ormsWriteNode` writes to every target node it can reach and reports success
  if at least one succeeded — matching the shipped behaviour of ignoring
  individual node failures.
- A missing node is not an error. Reads return empty, writes return success;
  the HAL is a best-effort knob pusher, not a transaction.

## Testing without a device

`io_shim` routes every absolute path through a process-global root override.
Tests set it to a temp directory, build a fake `/sys` tree (including the
per-core cpufreq symlinks), and exercise the real code paths against it. The
fixture lock recovers from poisoning so one failing test does not cascade.

```sh
cd rust && cargo test --offline
```

## What is not done here

- No repack, no flash. `scripts/stage_vendor.sh` copies the artifact into the
  staging tree only.
- The init rc and the VINTF fragment are the stock ones; they already match
  what this binary exposes.