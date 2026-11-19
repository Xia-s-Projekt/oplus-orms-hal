#!/bin/bash
# Stage the built OPlus Orms HAL into a target vendor tree.
# Set ROM_TREE to the unpacked vendor tree root; the default is a sibling of
# this checkout.
set -euo pipefail

W=$(cd "$(dirname "$0")/.." && pwd)
ROM_TREE=${ROM_TREE:-"$W/../../../erofs"}
SRC="$W/rust/target/aarch64-linux-android/release/vendor.oplus.hardware.ormsHalService-aidl-service"

GRN=$'\033[32m'; RED=$'\033[31m'; NC=$'\033[0m'

[[ -f "$SRC" ]] || { echo -e "${RED}missing $SRC: run scripts/build_rust.sh first${NC}"; exit 1; }

live="$ROM_TREE/vendor/bin/hw/vendor.oplus.hardware.ormsHalService-aidl-service"
install -m 0755 "$SRC" "$live"
echo -e "${GRN}staged${NC} $live"
echo "done"
