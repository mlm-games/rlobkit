//! JNI implementations for `RlobKitMainActivity`.
//!
//! These symbols are referenced by the Kotlin `RlobKitMainActivity` and must
//! be linked into any app that uses that Activity class.

use crate::insets::{WindowInsets, set_window_insets};
use jni::objects::JByteArray;
use jni::sys::{jbyteArray, jfloat, jobject};
use jni::{EnvUnowned, errors::ThrowRuntimeExAndDefault};

/// Called by `RlobKitMainActivity`'s `OnApplyWindowInsetsListener`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitMainActivity_nativeOnWindowInsets(
    _env: EnvUnowned,
    _this: jobject,
    top_px: jfloat,
    bottom_px: jfloat,
    left_px: jfloat,
    right_px: jfloat,
    ime_bottom_px: jfloat,
) {
    set_window_insets(WindowInsets {
        top: top_px,
        bottom: bottom_px,
        left: left_px,
        right: right_px,
        ime_bottom: ime_bottom_px,
    });
}

/// Optional JNI hook called by `RlobKitMainActivity` in addition to the
/// file-based `pending_intent` protocol.
///
/// Apps that need real-time delivery (e.g. my retorrent repo before switching to
/// file polling) can implement this.  It is **not** declared in the shared
/// Activity by default — apps opt in by adding an `external fun` declaration
/// in their own Activity subclass.
///
/// If declared, the symbol must exist or the Activity class won't load.
/// To avoid that, the shared Activity calls it via reflection when present.
pub fn default_native_on_new_intent(env: &mut jni::EnvUnowned, data: jbyteArray) {
    env.with_env(|env| {
        let array = unsafe { JByteArray::from_raw(env, data) };
        let bytes = env.convert_byte_array(&array)?;
        log::info!(
            "rlobkit_app_events::jni: nativeOnNewIntent ({} bytes)",
            bytes.len()
        );
        crate::push_intent(bytes);
        Ok::<_, jni::errors::Error>(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
