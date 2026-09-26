use std::sync::atomic::{AtomicBool, Ordering};

static IMMERSIVE: AtomicBool = AtomicBool::new(false);

pub fn is_immersive() -> bool {
    IMMERSIVE.load(Ordering::Relaxed)
}

/// Hide the system bars and keep them hidden until a swipe reveals them
/// transiently. On Android the running `RlobKitMainActivity` applies it; a
/// request that lands before the activity finishes `onCreate` is picked up
/// there instead.
#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub fn set_immersive_sticky(hide: bool) {
    IMMERSIVE.store(hide, Ordering::Relaxed);
    post_to_activity(hide);
}

#[cfg(any(not(feature = "jni-bridge"), not(target_os = "android")))]
pub fn set_immersive_sticky(hide: bool) {
    IMMERSIVE.store(hide, Ordering::Relaxed);
}

/// Show or hide the system bars, the inverse of [`set_immersive_sticky`].
pub fn set_system_bars_visible(visible: bool) {
    set_immersive_sticky(!visible);
}

/// The Activity owns the window, so the request has to reach the Activity
/// instance. `jni_min_helper::android_context()` hands out the
/// `android.app.Application` instead, which JNI rejects for an `Activity`
/// parameter, so ask the shared Activity class to route it.
#[cfg(all(feature = "jni-bridge", target_os = "android"))]
fn post_to_activity(hide: bool) {
    use jni::{jni_sig, jni_str, objects::JValue};
    let result = jni_min_helper::jni_with_env(|env| {
        env.call_static_method(
            jni_str!("rust/rlobkit/RlobKitMainActivity"),
            jni_str!("postImmersiveSticky"),
            jni_sig!("(Z)V"),
            &[JValue::Bool(hide)],
        )?;
        Ok(())
    });
    if let Err(e) = result {
        log::warn!("rlobkit_app_events::system_bars: post failed: {e}");
    }
}
