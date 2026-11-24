// SPDX-License-Identifier: MPL-2.0
//! Accelerator contract-version compatibility.
//!
//! The type and its rule live in `enaction-version`, the workspace's proved
//! leaf crate: a host accepts the same major and an operation minor version
//! no newer than itself. Creusot proves that crate as a whole.

pub use enaction_version::ContractVersion;
