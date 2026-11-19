#!/bin/bash
# Build the Rust OPlus Orms HAL for aarch64 Android, then verify the artifact.
# Does not touch the device or any system image.
#
# Output:
#   rust/target/aarch64-linux-android/release/vendor.oplus.hardware.ormsHalService-aidl-service
set -euo pipefail

W=$(cd "$(dirname "$0")/.." && pwd)
cd "$W/rust"

# Toolchain and platform library locations are configurable so the checkout is
# portable. Defaults match the layout used by this project.
ORMS_NDK=${ORMS_NDK:-/opt/android-sdk/ndk/27.0.12077973/toolchains/llvm/prebuilt/linux-x86_64}
ORMS_LIBBINDER_NDK_SO=${ORMS_LIBBINDER_NDK_SO:-"$W/../../../erofs/system/system/lib64/libbinder_ndk.so"}

export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ORMS_NDK/bin/aarch64-linux-android35-clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,--no-undefined -C link-arg=-Wl,-z,relro,-z,now -C link-arg=-Wl,--build-id=none -C link-arg=-llog -C link-arg=$ORMS_LIBBINDER_NDK_SO"

echo "=== host unit tests (node writers, node remap, domain discovery) ==="
cargo test --offline
echo

echo "=== cross-build aarch64-linux-android (release) ==="
cargo build --release --offline --target aarch64-linux-android --bins

OUT=target/aarch64-linux-android/release
SRC="$OUT/orms_service"
DST="$OUT/vendor.oplus.hardware.ormsHalService-aidl-service"
[[ -f "$SRC" ]] || { echo "build produced no binary" >&2; exit 1; }
install -m 0755 "$SRC" "$DST"

echo
echo "=== built ==="
ls -la "$DST"
"$ORMS_NDK/bin/llvm-readelf" -d "$DST" | grep NEEDED
"$ORMS_NDK/bin/llvm-readelf" -p .comment "$DST" | head -5
echo BUILD_OK
