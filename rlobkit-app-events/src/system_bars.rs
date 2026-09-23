use std::sync::atomic::{AtomicBool, Ordering};

static IMMERSIVE: AtomicBool = AtomicBool::new(false);

pub fn is_immersive() -> bool {
    IMMERSIVE.load(Ordering::Relaxed)
}

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub fn set_immersive_sticky(hide: bool) {
    IMMERSIVE.store(hide, Ordering::Relaxed);
    post_to_activity("setImmersiveSticky", hide);
}

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub fn set_system_bars_visible(visible: bool) {
    IMMERSIVE.store(!visible, Ordering::Relaxed);
    post_to_activity("setSystemBarsVisible", visible);
}

#[cfg(any(not(feature = "jni-bridge"), not(target_os = "android")))]
pub fn set_immersive_sticky(_hide: bool) {}

#[cfg(any(not(feature = "jni-bridge"), not(target_os = "android")))]
pub fn set_system_bars_visible(_visible: bool) {}

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
fn post_to_activity(name: &str, value: bool) {
    use jni::{jni_sig, jni_str, objects::JValue};
    let result = jni_min_helper::jni_with_env(|env| {
        let context = env.new_local_ref(jni_min_helper::android_context())?;
        let name = env.new_string(name)?;
        let runnable = env.new_object(
            jni_str!("rust/rlobkit/SystemBarsRunnable"),
            jni_sig!("(Landroid/app/Activity;Ljava/lang/String;Z)V"),
            &[
                JValue::Object(&context),
                JValue::Object(&name),
                JValue::Bool(value),
            ],
        )?;
        env.call_method(
            &context,
            jni_str!("runOnUiThread"),
            jni_sig!("(Ljava/lang/Runnable;)V"),
            &[JValue::Object(&runnable)],
        )?;
        Ok(())
    });
    if let Err(e) = result {
        log::warn!("rlobkit_app_events::system_bars: post failed: {e}");
    }
}
