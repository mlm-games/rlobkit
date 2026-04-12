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
pub use android::{
    helper_activity_available_for_host, on_activity_result, on_activity_result_from_intent,
    take_writable_fd_for_uri,
};

pub use mode::RlobKitMode;
pub use picker::{OpenDirectoryOptions, OpenFileOptions, RlobKit, SaveFileOptions};
pub use rlobkit_core::{PlatformDirectory, PlatformFile, RlobKitError};
pub use types::RlobKitType;
