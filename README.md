# OplusOrms

A Rust implementation of the OPlus ORMS (OPlus Resource Management Service)
AIDL HAL.

## What it does

Functionally, ORMS is the scheduler and DVFS control surface. The framework uses
it to read available CPU and GPU frequency tables and to write scheduler,
boost-frequency and thermal-tuning values into kernel nodes.

`OplusOrms` implements the
`vendor.oplus.hardware.ormsHalService.IOrmsAidlHalService` interface. It exposes
scheduler and DVFS tuning nodes over binder: callers can write a value into a
kernel node, read a value back, or query an available-frequency table. Node
paths are resolved against the nodes present on the target kernel, with a
fallback mapping for equivalent nodes published under different names.

The binder layer uses the NDK C API directly. The crate has no C/C++
dependencies and links only against platform libraries (`libc`, `liblog`,
`libdl`, and `libbinder_ndk`).

## Layout

```
aidl/vendor/oplus/hardware/ormsHalService/IOrmsAidlHalService.aidl
    AIDL interface definition.

rust/
    Cargo.toml, Cargo.lock, rust-toolchain.toml, .cargo/config.toml
    src/lib.rs                  crate root; re-exports the host-testable core
    src/core.rs                 method bodies: node readers and writers
    src/node_remap.rs           node discovery and name fallback mapping
    src/io_shim.rs              filesystem access with a test-root override
    src/wire.rs                 AIDL parcel read/write and status handling
    src/hal.rs                  AIBinder class definition and transaction dispatch
    src/bin/orms_service.rs     entry point: thread pool and service registration

scripts/
    build_rust.sh               host tests, release cross-build, artifact verification
    stage_vendor.sh             copy the built binary into a target vendor tree
```

## Requirements

- The Rust toolchain pinned in `rust/rust-toolchain.toml`, with the
  `aarch64-linux-android` target installed.
- Android NDK. `scripts/build_rust.sh` defaults to NDK 27.0.12077973; set
  `ORMS_NDK` to the NDK's `toolchains/llvm/prebuilt/linux-x86_64` directory if
  it is installed elsewhere.
- A platform `libbinder_ndk.so` to link against. Set `ORMS_LIBBINDER_NDK_SO` to
  its path. The build script defaults to a relative path to a sibling vendor
  tree of this checkout.

## Prebuilt

[Get Prebuilt](https://github.com/Xia-s-Projekt/oplus-orms-hal/releases/latest) — AArch64 ODM archive containing the service under `odm/bin/hw/`, its init rc under `odm/etc/init/`, and the VINTF manifest under `odm/etc/vintf/manifest/`.

## Central debug logging

All verbose logging in this repo is gated by one property shared across the
Rianixia HALs:

```
persist.vendor.rianixia.debug   # vendor alias, checked FIRST
persist.sys.rianixia.debug      # canonical property
```

Set either to `1` or `true` (case-insensitive) to enable, for example:

```sh
adb root
adb shell setprop persist.sys.rianixia.debug 1
```

Unset, `0`, `false` or any other value disables verbose logging. Production
devices are quiet by default. ERROR/FATAL lines for genuine failures are
always emitted. Diagnostic-only hooks (instance-suffix overrides, dump
triggers, runtime log-level switches) are honoured only while this debug prop
is enabled.

## Build

```sh
bash scripts/build_rust.sh
```

This runs the host unit tests, builds the release binary for
`aarch64-linux-android`, renames it to its installed filename, and prints the
dynamic dependencies and compiler metadata. Output is written to
`rust/target/aarch64-linux-android/release/`.

To run only the host tests:

```sh
cd rust && cargo test --offline
```

The tests route filesystem access through a temporary root, so node discovery
and the read/write methods are exercised in a host environment without target
hardware.

## Staging

```sh
ROM_TREE=/path/to/target/tree bash scripts/stage_vendor.sh
```

`stage_vendor.sh` copies the built binary to
`vendor/bin/hw/vendor.oplus.hardware.ormsHalService-aidl-service` under
`ROM_TREE`. It does not build images, flash devices, or modify filesystem
configuration files. The init rc and VINTF manifest that declare the service are
expected to be present in the target tree and are left unchanged.
