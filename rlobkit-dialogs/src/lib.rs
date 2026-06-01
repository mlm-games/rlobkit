//! rlobkit-dialogs: unified file/directory picker and save dialog API.

pub mod blocking;
pub mod mode;
pub mod picker;
pub mod types;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod desktop;

#[cfg(target_arch = "wasm32")]
mod desktop;

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
pub use android::{
    helper_activity_available_for_host, init_with_context, on_activity_result,
    on_activity_result_from_intent, take_writable_fd_for_uri,
};

#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_arch = "wasm32"
))]
pub use blocking::{
    blocking_open_file, blocking_pick_directory, blocking_pick_files, blocking_save_file,
};

pub use mode::RlobKitMode;
pub use picker::{OpenDirectoryOptions, OpenFileOptions, RlobKit, SaveFileOptions};
pub use rlobkit_core::{PlatformDirectory, PlatformFile, RlobKitError};
pub use types::RlobKitType;

/// Register platform-specific I/O callbacks. Must be called once at app
/// startup on Android before any `PlatformFile::read_bytes` or
/// `PlatformFile::write_bytes` is invoked on a URI-backed file. No-op on
/// other platforms.
pub fn init() {
    #[cfg(target_os = "android")]
    {
        android::init();
    }
}

pub fn init_with_android_context(
    #[cfg(target_os = "android")] vm: *mut std::ffi::c_void,
    #[cfg(target_os = "android")] context: *mut std::ffi::c_void,
) {
    #[cfg(target_os = "android")]
    {
        unsafe { android::init_with_context(vm, context) };
    }
}
