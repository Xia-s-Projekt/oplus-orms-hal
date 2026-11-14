#!/bin/bash
# Stage the Rust port of the OPlus Orms HAL into the unpacked vendor tree.
#
# Staged:
#   erofs/vendor/bin/hw/vendor.oplus.hardware.ormsHalService-aidl-service
#
# The init rc (erofs/vendor/etc/init/ormsHalService-default.rc, class hal,
# user system, group system readproc root) and the VINTF fragment
# (erofs/vendor/etc/vintf/manifest/ormsHalService-default.xml) point at this
# same path and stay unchanged.
#
# The binary lands at the same path as the build it replaces, so the existing
# fs_config / file_contexts entries still apply and nothing is regenerated
# (that only happens at repack time).
#
# This only touches files in the unpacked tree. It does NOT repack, does NOT
# flash, and does NOT touch fs_config or file_contexts.
set -euo pipefail

W=$(cd "$(dirname "$0")/.." && pwd)
ROM_TREE=${ROM_TREE:-/mnt/project/RomTools/ROM_WORKDIR/oos/erofs}
SRC="$W/rust/target/aarch64-linux-android/release/vendor.oplus.hardware.ormsHalService-aidl-service"

GRN=$'\033[32m'; RED=$'\033[31m'; NC=$'\033[0m'

[[ -f "$SRC" ]] || { echo -e "${RED}missing $SRC: run scripts/build_rust.sh first${NC}"; exit 1; }

live="$ROM_TREE/vendor/bin/hw/vendor.oplus.hardware.ormsHalService-aidl-service"
install -m 0755 "$SRC" "$live"
echo -e "${GRN}staged${NC} $live"
echo "done"
