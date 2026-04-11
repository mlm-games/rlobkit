//! rlobkit-dialogs: unified file/directory picker and save dialog API.

pub mod mode;
pub mod picker;
pub mod types;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod desktop;

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
pub use android::{on_activity_result, on_activity_result_from_intent};

pub use mode::RlobKitMode;
pub use picker::RlobKit;
pub use types::RlobKitType;
