//! Android platform glue that winit does not already provide.
//!
//! The Kotlin `RlobKitMainActivity` (or a custom subclass) is the Android side of
//! all of this. It pushes state in over JNI; the modules here own the state and
//! expose it as plain Rust.
//!
//! ## What is here, and why not winit
//!
//! - [`insets`] — winit's Android insets are unimplemented (it logs a `TODO` and
//!   drops the notification), so cutout, gesture-area and IME geometry has to come
//!   from here.
//! - [`theme`] — the wallpaper-derived Material 3 palette. winit's `Theme` is only
//!   a light/dark enum.
//! - [`system_bars`] — system bar visibility and the edge-to-edge policy; winit
//!   exposes neither on Android.
//! - [`back`] — back invocation and predictive-back handling; winit has neither.
//! - [`memory`] — `onTrimMemory` / `onLowMemory`, which winit does not surface.
//! - [`intents`] — incoming shares. Captured as URI-backed files rather than
//!   bytes, so a large video is not read on the main thread. Call
//!   [`intents::take_pending_intent`] before the UI loop starts and
//!   [`intents::drain_intents`] each frame.
//!
//! ## What is deliberately absent
//!
//! Light/dark mode and the Activity lifecycle are **not** here. winit reports the
//! first as `Window::system_theme()` and the second as `ApplicationHandler` /
//! window events, and every app using this crate already runs on winit. Anything
//! winit can answer is left to winit; duplicating it here would only create a
//! second source of truth that can disagree.
//!
//! ## Feature flags
//!
//! Everything that reaches the Activity over JNI needs the `jni-bridge` feature.
//! Without it the shared Activity still works — it guards its native calls — but
//! nothing is pushed, so every `last_*`/`system_*` accessor here stays `None`.
//!
//! Enabling the feature is not by itself enough: without it the Activity's native
//! calls all fail with `UnsatisfiedLinkError`, which it tolerates, so nothing is
//! pushed and nothing reports why. [`jni::verify_linked`] checks for that.
//!
//! ## Example
//!
//! ```ignore
//! use rlobkit_app_events::{insets, intents, memory};
//!
//! fn android_main(android_app: AndroidApp) {
//!     rlobkit_app_events::jni::verify_linked();
//!
//!     insets::set_on_insets(|i| println!("safe area {:?}", i.safe()));
//!     memory::set_on_memory_pressure(|p| println!("{p:?}"));
//!
//!     let data_dir = android_app.internal_data_path();
//!     if let Some(intent) = data_dir.and_then(|d| intents::take_pending_intent(d)) {
//!         for file in &intent.files {
//!             println!("{} ({:?})", file.name(), file.mime_type());
//!         }
//!     }
//!     // ...
//! }
//! ```

pub mod back;
pub mod insets;
pub mod intents;
pub mod memory;
pub mod system_bars;
pub mod theme;
pub mod vibrator;

mod subscriber;

#[cfg(all(feature = "jni-bridge", target_os = "android"))]
pub mod jni;

#[cfg(target_os = "android")]
pub mod android_log;

pub use intents::AppIntent;
