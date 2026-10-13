//! Prints the Kotlin `BleUuids` source from the Rust-owned UUID constants.
//!
//! The Plain-owned GATT identifiers live here in Rust (single source of
//! truth). The App's `BleUuids.kt` is generated from this module so the two
//! sides can never drift. Regenerate with:
//!
//! ```text
//! cargo test -p plain-rs --features ble ble::print::regenerate_kotlin_uuids -- --ignored --nocapture
//! ```
//!
//! Output is byte-stable when the constants are unchanged; `guard` fails when
//! the committed Kotlin file goes stale.

use crate::protocols::ble::{CCC_DESCRIPTOR_UUID, HTTP_CHAR_UUID, NEARBY_CHAR_UUID, SERVICE_UUID};

/// The generated Kotlin `BleUuids` object, header included.
pub fn kotlin_uuids_source() -> String {
    format!(
        r#"// GENERATED from plain-rs protocols/ble UUID constants. Do not edit by hand.
// Regenerate: cargo test -p plain-rs --features ble ble::print::regenerate_kotlin_uuids -- --ignored --nocapture
package com.ismartcoding.plain.ble

object BleUuids {{
    const val SERVICE_UUID = "{SERVICE_UUID}"
    const val NEARBY_CHAR_UUID = "{NEARBY_CHAR_UUID}"
    const val HTTP_CHAR_UUID = "{HTTP_CHAR_UUID}"
    const val CCC_DESCRIPTOR_UUID = "{CCC_DESCRIPTOR_UUID}"
}}
"#
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/protocols/ble/print.rs"]
mod tests;
