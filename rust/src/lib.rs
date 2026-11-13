// The cross-platform half of the OPlus Orms HAL port.
//
// The node writers, the boost bookkeeping and the node remap live here so
// they are unit-testable on the host against one fake sysfs tree. The
// binder-facing half (wire.rs / hal.rs) is an Android-only binary in
// src/bin/orms_service.rs.

pub mod core;
pub mod io_shim;
pub mod node_remap;
