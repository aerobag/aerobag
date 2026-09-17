// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Frozen cloud payloads, not runtime model serialization. Change persisted
//! meaning by adding an account successor; keep runtime conversion explicit.

pub(super) mod flight_plan;
pub(super) mod preferences;

// These macros define wire-only types and exhaustive conversions. Runtime
// fields/variants cannot silently become part of the stored contract.
macro_rules! wire_enum {
    ($(#[$attr:meta])* $name:ident => $runtime:path { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[cfg_attr(test, derive(schemars::JsonSchema))]
        $(#[$attr])*
        pub(crate) enum $name { $($variant),+ }
        impl From<$runtime> for $name {
            fn from(value: $runtime) -> Self {
                use $runtime as Runtime;
                match value { $(Runtime::$variant => Self::$variant),+ }
            }
        }
        impl From<$name> for $runtime {
            fn from(value: $name) -> Self {
                match value { $($name::$variant => Self::$variant),+ }
            }
        }
    };
}
pub(super) use wire_enum;
