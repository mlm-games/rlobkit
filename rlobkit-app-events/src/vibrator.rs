//! Device vibration, driven through the shared Activity's vibrator.

/// Vibrate the device for `duration_ms`. The two rumble channels mix the
/// way Aurora's device haptics do (`0.6 * low + 0.4 * high`), because
/// Android exposes one device vibrator rather than per-motor force
/// feedback. A zero duration or zero strength stops vibration instead.
/// No-op off Android or without the `jni-bridge` feature.
pub fn rumble(low: f32, high: f32, duration_ms: u32) {
    let strength = (0.6 * low + 0.4 * high).clamp(0.0, 1.0);
    if duration_ms > 0 && strength > 0.0 {
        #[cfg(all(feature = "jni-bridge", target_os = "android"))]
        post_rumble(
            duration_ms,
            ((strength * 255.0).round() as i32).clamp(1, 255),
        );
    } else {
        stop();
    }
}

/// Stop any vibration [`rumble`] started. No-op off Android or without the
/// `jni-bridge` feature.
pub fn stop() {
    #[cfg(all(feature = "jni-bridge", target_os = "android"))]
    post_stop();
}

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
fn post_rumble(duration_ms: u32, amplitude: i32) {
    use jni::objects::JValue;
    use jni::{jni_sig, jni_str};
    let result = jni_min_helper::jni_with_env(|env| {
        env.call_static_method(
            jni_str!("rust/rlobkit/RlobKitMainActivity"),
            jni_str!("rumble"),
            jni_sig!("(JI)V"),
            &[JValue::Long(i64::from(duration_ms)), JValue::Int(amplitude)],
        )?;
        Ok(())
    });
    if let Err(e) = result {
        log::warn!("rlobkit_app_events::vibrator: rumble failed: {e}");
    }
}

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
fn post_stop() {
    use jni::{jni_sig, jni_str};
    let result = jni_min_helper::jni_with_env(|env| {
        env.call_static_method(
            jni_str!("rust/rlobkit/RlobKitMainActivity"),
            jni_str!("rumbleStop"),
            jni_sig!("()V"),
            &[],
        )?;
        Ok(())
    });
    if let Err(e) = result {
        log::warn!("rlobkit_app_events::vibrator: stop failed: {e}");
    }
}
