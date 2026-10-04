# rlobkit

Partially cross-platform file-picking toolkit for Rust. Inspired by [FileKit](https://github.com/vinceglb/FileKit) for Kotlin Multiplatform.

Workspace crates:

| Crate | Role |
|-------|------|
| `rlobkit-core` | `PlatformFile` / `PlatformDirectory`, I/O, paths helpers, errors |
| `rlobkit-dialogs` | Open / save / directory pickers (async + desktop blocking helpers) |
| `rlobkit-image` | Image resize/compress (JPEG/PNG/WebP) |
| `rlobkit-app-events` | Android glue winit lacks: insets, dynamic colors, system bars, back, memory pressure, intents |

License: MIT  
Version: see workspace `Cargo.toml` (currently 0.5.x)

Android integration works but is constrained by OS/platform APIs (not a Rust limitation). See [yadaw](https://github.com/mlm-games/yadaw) for a real app using this stack.

## Platform support

| Capability | Desktop (Win/macOS/Linux) | Android | WASM |
|------------|---------------------------|---------|------|
| Open file(s) | rfd | SAF / system picker via helper activity | rfd (in-memory blobs) |
| Save file | rfd | CREATE_DOCUMENT + writable FD | rfd write |
| Directory pick | rfd | SAF tree URI | not supported |
| Path-backed I/O | yes | URI via registered JNI I/O | N/A (bytes only) |
| Blocking picker helpers | yes | — | — |
| List directory files | path | not yet for SAF URI | — |
| Dynamic (wallpaper) colors | — | Material 3 palette, Android 12+ | — |
| Edge-to-edge insets / cutout / gestures | winit desktop | yes (winit Android lacks it) | — |
| Back + predictive back | winit desktop | yes | — |
| Image compress | yes | yes | yes (if you pull the crate) |

## rlobkit-core

- `PlatformFile`: name, optional size/mime, source = path | Android URI | bytes
- `read_bytes` / `write_bytes` / `write_string`; async read with optional `tokio-runtime` (default)
- Android URI I/O requires `rlobkit_dialogs::init()` (or `set_android_io`) so read/write callbacks are registered
- `PlatformDirectory`: path or Android URI; `file(name)`, `list_files()` on paths
- Path helpers: `files_dir`, `cache_dir`, `home_dir`, `downloads_dir`, `pictures_dir` (non-WASM)
- `mime_to_extension` via mime_guess (+ small custom map)

## rlobkit-dialogs

Unified API:

```rust
use rlobkit_dialogs::{
    init, OpenFileOptions, OpenDirectoryOptions, SaveFileOptions,
    RlobKit, RlobKitMode, RlobKitType,
};

// Android: once at startup
init();

// Open one or more files
let files = RlobKit::open_file_picker(OpenFileOptions {
    file_type: RlobKitType::Image, // Any | Image | Video | ImageAndVideo | Custom { .. }
    mode: RlobKitMode::Multiple { limit: None },
    title: Some("Choose".into()),
    initial_directory: None,
}).await?;

// Convenience
let one = RlobKit::open_single_file(RlobKitType::Any).await?;

// Directory
let dir = RlobKit::open_directory_picker(OpenDirectoryOptions::default()).await?;

// Save + write bytes in one step where supported
let saved = RlobKit::save_bytes(
    SaveFileOptions {
        suggested_name: Some("export.png".into()),
        ..Default::default()
    },
    &bytes,
).await?;
```

Desktop also exports blocking helpers (via `futures-lite`):

- `blocking_open_file`, `blocking_pick_files`, `blocking_save_file`, `blocking_pick_directory`

Copy helpers: `write_file_from_path`, `read_file_to_path` (filesystem on desktop/Android path/URI backends; unsupported on WASM).

### Android details

- Helper `RlobKitPickerActivity` launches the system picker and returns results over JNI
- Persistable URI permissions and self grants where the system provides them
- Create-document path can attach a writable FD for reliable writes
- Optional `init_with_android_context` / activity-result hooks for custom hosts
- cargo-rapk metadata registers the picker activity Java sources

Call `init()` before reading/writing URI-backed `PlatformFile`s.

### WASM details

- Picked files become in-memory `PlatformFile` (bytes)
- Directory picker and path copy APIs return `UnsupportedOperation`
- Save can write dialog data when `SaveFileOptions.data` is set (used by `save_bytes`)

## rlobkit-image

```rust
use rlobkit_image::{compress_image, CompressOptions, CompressFormat};

let out = compress_image(
    &input_bytes,
    CompressOptions {
        quality: 80,
        max_width: 1920,
        max_height: 1080,
        format: CompressFormat::Jpeg, // Jpeg | Png | WebP
    },
)?;
```

Uses the `image` crate with jpeg/png/webp features. Resize uses Lanczos3 when larger than max dimensions.

## rlobkit-app-events

Android glue for what winit cannot do. Apps that use this crate run on winit, so
anything winit already answers is left to winit rather than duplicated here — two
sources for the same fact can only disagree.

**Here because winit has no equivalent** (optional `jni-bridge` feature):

- **Insets** — winit's Android insets are unimplemented (it receives the
  notification and logs a `TODO`), so bar/IME rectangles, display cutout, gesture
  exclusion areas, per-region visibility and IME animation progress come from
  here. `set_on_insets`, `last_window_insets`
- **Dynamic theme** — the wallpaper-derived Material 3 palette. winit's `Theme` is
  only a light/dark enum. `last_theme`, `set_on_theme`,
  `dynamic_colors_available`
- **System bars / edge-to-edge** — bar visibility and the `EdgeToEdgeMode` policy
  (`Disabled` / `Enabled` / `Immersive`); winit exposes neither on Android
- **Back** — `set_on_back` decides what a back press does (consume it, or let the
  system go home); `set_on_back_event` observes predictive-back gesture phases
- **Memory pressure** — `onTrimMemory` / `onLowMemory`, which winit does not surface
- **Intents** — `ACTION_VIEW` / `ACTION_SEND` / `ACTION_SEND_MULTIPLE` captured as
  URI-backed records, not bytes; `take_pending_intent` at startup and
  `drain_intents` per frame
- Shared Kotlin: `RlobKitMainActivity`, `RlobKitIntentBridge`

Reading a captured file's contents goes through `PlatformFile::read_bytes`, so it
needs `rlobkit_dialogs::init()` registered as for any other URI-backed file.

**Deliberately absent, because winit covers it:** light/dark mode
(`Window::system_theme()`) and the Activity lifecycle / window focus
(`ApplicationHandler::{resumed, suspended}` plus the `Focused` / `Resumed` window
events).

### Enabling the JNI bridge

Apps that want state pushed from the Activity need the `jni-bridge` feature. Any
app that calls into this crate already keeps the Activity's native entry points
linked, so there is nothing extra to wire up; `jni::verify_linked()` is available
for the one failure that is otherwise silent — an app that receives no inset,
palette or back event, and no error explaining why.

Host apps are responsible for the manifest permissions they use: `VIBRATE` for
`vibrator`, and `android:enableOnBackInvokedCallback="true"` to get predictive
back on Android 13 and 14. Predictive-back *progress* additionally needs AndroidX:
the platform SDK has no public way to register `OnBackAnimationCallback`.

Useful when embedding in a NativeActivity / custom activity stack (e.g. repose-platform, yadaw).

## Workspace usage

```toml
[dependencies]
rlobkit-core = "0.5"
rlobkit-dialogs = "0.5"
rlobkit-image = "0.5"          # optional
rlobkit-app-events = "0.5"     # optional, Android
```

Desktop pickers depend on [rfd](https://github.com/PolyMeilex/rfd). Android needs JNI + the bundled Java/Kotlin helper sources in the APK (see crate `package.metadata.android.cargo_rapk`).

## Limitations

- Android directory listing over SAF URI is not implemented yet
- WASM has no directory picker and no filesystem path copy (uses rfd, which doesn't support it yet)
- Some Android save/open behaviors depend on OEM document UI and grant flags
- Not yet a full mirror of every FileKit KMP API (only pickers + portable file handles + few extras for now)

## Related

- Example consumer: [yadaw](https://github.com/mlm-games/yadaw)
- UI toolkit often paired with: [repose](https://github.com/mlm-games/repose)
