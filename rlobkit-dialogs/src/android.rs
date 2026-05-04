use crate::RlobKitMode;
use crate::picker::{OpenDirectoryOptions, OpenFileOptions, SaveFileOptions};
use jni::{
    Env, EnvUnowned,
    errors::Error as JniError,
    jni_sig, jni_str,
    objects::{JByteArray, JObject, JObjectArray, JString, JValue},
    refs::Global,
};
use jni_min_helper::{android_context, jni_with_env};
use rlobkit_core::{PlatformDirectory, PlatformFile, RlobKitError};
use std::collections::HashMap;
use std::os::fd::FromRawFd;
use std::path::Path;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

const EXTRA_OPEN_FD: &str = "rust.rlobkit.extra.OPEN_FD";

const REQUEST_OPEN_SINGLE: i32 = 41001;
const REQUEST_OPEN_MULTI: i32 = 41002;
const REQUEST_OPEN_DIRECTORY: i32 = 41003;
const REQUEST_CREATE_DOCUMENT: i32 = 41004;

const RESULT_OK: i32 = -1;
const WAIT_TIMEOUT: Duration = Duration::from_secs(120);

const FLAG_GRANT_READ_URI_PERMISSION: i32 = 1;
const FLAG_GRANT_WRITE_URI_PERMISSION: i32 = 2;
const FLAG_GRANT_PERSISTABLE_URI_PERMISSION: i32 = 64;
const FLAG_GRANT_PREFIX_URI_PERMISSION: i32 = 128;
const FLAG_ACTIVITY_NEW_TASK: i32 = 0x10000000;

const EXTRA_ALLOW_MULTIPLE: &str = "android.intent.extra.ALLOW_MULTIPLE";
const EXTRA_MIME_TYPES: &str = "android.intent.extra.MIME_TYPES";
const EXTRA_TITLE: &str = "android.intent.extra.TITLE";

const ACTION_OPEN_DOCUMENT: &str = "android.intent.action.OPEN_DOCUMENT";
const ACTION_OPEN_DOCUMENT_TREE: &str = "android.intent.action.OPEN_DOCUMENT_TREE";
const ACTION_CREATE_DOCUMENT: &str = "android.intent.action.CREATE_DOCUMENT";
const CATEGORY_OPENABLE: &str = "android.intent.category.OPENABLE";

const RLOBKIT_HELPER_LAUNCH_ACTION: &str = "rust.rlobkit.action.LAUNCH_PICKER";
const RLOBKIT_HELPER_ACTIVITY_NAME: &str = "rust.rlobkit.RlobKitPickerActivity";
const RLOBKIT_EXTRA_TARGET_INTENT: &str = "rust.rlobkit.extra.TARGET_INTENT";
const RLOBKIT_EXTRA_REQUEST_CODE: &str = "rust.rlobkit.extra.REQUEST_CODE";

#[derive(Default)]
struct PendingRequest {
    request_code: Option<i32>,
    result: Option<ActivityResult>,
}

#[derive(Debug, Clone)]
struct ActivityResult {
    result_code: i32,
    data_uri: Option<String>,
    clip_uris: Vec<String>,
    grant_flags: i32,
    open_fd: Option<i32>,
}

fn pending_state() -> &'static (Mutex<PendingRequest>, Condvar) {
    static STATE: OnceLock<(Mutex<PendingRequest>, Condvar)> = OnceLock::new();
    STATE.get_or_init(|| (Mutex::new(PendingRequest::default()), Condvar::new()))
}

fn saver_fd_state() -> &'static Mutex<HashMap<String, i32>> {
    static STATE: OnceLock<Mutex<HashMap<String, i32>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn close_raw_fd(fd: i32) {
    if fd < 0 {
        return;
    }
    unsafe {
        let _ = std::fs::File::from_raw_fd(fd);
    }
}

fn stash_writable_fd_for_uri(uri: &str, fd: i32) {
    if fd < 0 {
        return;
    }

    if let Ok(mut map) = saver_fd_state().lock() {
        if let Some(old_fd) = map.insert(uri.to_string(), fd) {
            close_raw_fd(old_fd);
        }
        log::info!("rlobkit-dialogs: stashed writable fd for uri={uri}");
    } else {
        close_raw_fd(fd);
        log::warn!("rlobkit-dialogs: failed to stash writable fd for uri={uri}");
    }
}

pub fn take_writable_fd_for_uri(uri: &str) -> Option<i32> {
    saver_fd_state().lock().ok()?.remove(uri)
}

fn map_jni_error(error: JniError) -> RlobKitError {
    RlobKitError::UnsupportedOperation(format!("Android JNI error: {error}"))
}

fn with_android_env<T>(
    f: impl FnOnce(&mut Env<'_>) -> Result<T, JniError>,
) -> Result<T, RlobKitError> {
    jni_with_env(f).map_err(map_jni_error)
}

fn annotate_jni_error(env: &mut Env<'_>, stage: &'static str, error: JniError) -> JniError {
    match error {
        JniError::JavaException => {
            let detail = match env.exception_catch() {
                Err(caught) => caught.to_string(),
                Ok(()) => "Java exception (no detail available)".to_string(),
            };
            log::error!("rlobkit-dialogs: JNI stage {stage} failed: {detail}");
            JniError::MethodNotFound {
                name: stage.to_string(),
                sig: detail,
            }
        }
        other => {
            log::error!("rlobkit-dialogs: JNI stage {stage} failed: {other}");
            other
        }
    }
}

fn begin_request(request_code: i32) -> Result<(), RlobKitError> {
    let (lock, _) = pending_state();
    let mut guard = lock
        .lock()
        .map_err(|_| RlobKitError::UnsupportedOperation("Android picker lock poisoned".into()))?;

    if guard.request_code.is_some() {
        return Err(RlobKitError::UnsupportedOperation(
            "Another Android picker request is in progress".into(),
        ));
    }

    guard.request_code = Some(request_code);
    guard.result = None;
    log::info!("rlobkit-dialogs: begin_request code={request_code}");
    Ok(())
}

fn cancel_request() {
    let (lock, cvar) = pending_state();
    if let Ok(mut guard) = lock.lock() {
        log::warn!(
            "rlobkit-dialogs: cancel_request code={:?}",
            guard.request_code
        );
        guard.request_code = None;
        guard.result = None;
        cvar.notify_all();
    }
}

fn wait_for_result(request_code: i32) -> Result<ActivityResult, RlobKitError> {
    let (lock, cvar) = pending_state();
    let mut guard = lock
        .lock()
        .map_err(|_| RlobKitError::UnsupportedOperation("Android picker lock poisoned".into()))?;

    let deadline = Instant::now() + WAIT_TIMEOUT;

    loop {
        if let Some(result) = guard.result.take() {
            guard.request_code = None;
            log::info!("rlobkit-dialogs: got activity result code={request_code}");
            return Ok(result);
        }

        let now = Instant::now();
        if now >= deadline {
            guard.request_code = None;
            return Err(RlobKitError::UnsupportedOperation(
                "Timed out waiting for Android activity result from RlobKitPickerActivity".into(),
            ));
        }

        let timeout = deadline.saturating_duration_since(now);
        let (next_guard, _) = cvar.wait_timeout(guard, timeout).map_err(|_| {
            RlobKitError::UnsupportedOperation("Android picker condvar poisoned".into())
        })?;
        guard = next_guard;

        if guard.request_code != Some(request_code) {
            return Err(RlobKitError::UnsupportedOperation(
                "Android picker request state was reset unexpectedly".into(),
            ));
        }
    }
}

jni::bind_java_type! {
    ContentResolver => "android.content.ContentResolver",
    type_map = {
        Uri => "android.net.Uri",
    },
    methods {
        fn take_persistable_uri_permission(uri: Uri, flags: jint),
    }
}

jni::bind_java_type! {
    Intent => "android.content.Intent",
    type_map = {
        Uri => "android.net.Uri",
        ClipData => "android.content.ClipData",
    },
    constructors {
        fn new(action: JString),
    },
    methods {
        fn set_type(type_: JString) -> Intent,
        fn add_category(category: JString) -> Intent,
        fn put_extra_bool {
            name = "putExtra",
            sig = (name: JString, value: jboolean) -> Intent,
        },
        fn put_extra_string {
            name = "putExtra",
            sig = (name: JString, value: JString) -> Intent,
        },
        fn put_extra_string_array {
            name = "putExtra",
            sig = (name: JString, value: JString[]) -> Intent,
        },
        fn get_data() -> Uri,
        fn get_clip_data() -> ClipData,
        fn get_flags() -> jint,
    }
}

jni::bind_java_type! {
    Uri => "android.net.Uri",
    methods {
        fn to_java_string {
            name = "toString",
            sig = () -> JString,
        },
        static fn parse(text: JString) -> Uri,
    }
}

jni::bind_java_type! {
    ClipData => "android.content.ClipData",
    type_map = {
        ClipDataItem => "android.content.ClipData$Item",
    },
    methods {
        fn get_item_count() -> jint,
        fn get_item_at(index: jint) -> ClipDataItem,
    }
}

jni::bind_java_type! {
    ClipDataItem => "android.content.ClipData$Item",
    type_map = {
        Uri => "android.net.Uri",
    },
    methods {
        fn get_uri() -> Uri,
    }
}

fn current_context<'a>(env: &mut Env<'a>) -> Result<JObject<'a>, JniError> {
    env.new_local_ref(android_context())
}

fn string_array<'a>(
    env: &mut Env<'a>,
    values: &[&str],
) -> Result<JObjectArray<'a, JString<'a>>, JniError> {
    let array = JObjectArray::<JString>::new(env, values.len(), JString::null())?;
    for (idx, value) in values.iter().enumerate() {
        let value = JString::new(env, value)?;
        array.set_element(env, idx, value)?;
    }
    Ok(array)
}

fn put_mime_filters(
    env: &mut Env<'_>,
    intent: &Intent<'_>,
    mimes: &[&str],
) -> Result<(), JniError> {
    if mimes.is_empty() || (mimes.len() == 1 && mimes[0] == "*/*") {
        return Ok(());
    }

    let key = JString::new(env, EXTRA_MIME_TYPES)?;
    let values = string_array(env, mimes)?;
    let _ = intent.put_extra_string_array(env, key, values)?;
    Ok(())
}

fn mime_from_extension(extension: Option<&str>) -> &'static str {
    match extension
        .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
        .as_deref()
    {
        Some("wav") => "audio/wav",
        Some("flac") => "audio/flac",
        Some("ogg") => "audio/ogg",
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("aac") => "audio/aac",
        Some("mid") | Some("midi") => "audio/midi",
        _ => "application/octet-stream",
    }
}

fn helper_activity_available() -> bool {
    with_android_env(|env| {
        let context = env.new_local_ref(android_context())?;

        let package_name = env
            .call_method(
                &context,
                jni_str!("getPackageName"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )
            .map_err(|e| annotate_jni_error(env, "helper.package_name", e))?
            .l()
            .map_err(|e| annotate_jni_error(env, "helper.package_name.as_l", e))?;
        let package_name = JString::cast_local(env, package_name)?;
        let class_name = JString::new(env, RLOBKIT_HELPER_ACTIVITY_NAME)?;

        let component_name = env
            .new_object(
                jni_str!("android/content/ComponentName"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
                &[JValue::Object(&package_name), JValue::Object(&class_name)],
            )
            .map_err(|e| annotate_jni_error(env, "helper.component_name", e))?;

        let package_manager = env
            .call_method(
                &context,
                jni_str!("getPackageManager"),
                jni_sig!("()Landroid/content/pm/PackageManager;"),
                &[],
            )
            .map_err(|e| annotate_jni_error(env, "helper.package_manager", e))?
            .l()
            .map_err(|e| annotate_jni_error(env, "helper.package_manager.as_l", e))?;

        let activity_info = env.call_method(
            &package_manager,
            jni_str!("getActivityInfo"),
            jni_sig!("(Landroid/content/ComponentName;I)Landroid/content/pm/ActivityInfo;"),
            &[JValue::Object(&component_name), JValue::Int(0)],
        );

        match activity_info {
            Ok(info) => Ok(!info.l()?.is_null()),
            Err(JniError::JavaException) => {
                let _ = env.exception_catch();
                Ok(false)
            }
            Err(error) => Err(error),
        }
    })
    .unwrap_or(false)
}

pub fn helper_activity_available_for_host() -> bool {
    helper_activity_available()
}

fn best_effort_take_persistable_uri_permission(
    env: &mut Env<'_>,
    resolver: &JObject<'_>,
    uri: &JObject<'_>,
    flags: i32,
    content_uri: &str,
) {
    let grant_result = env.call_method(
        resolver,
        jni_str!("takePersistableUriPermission"),
        jni_sig!("(Landroid/net/Uri;I)V"),
        &[JValue::Object(uri), JValue::Int(flags)],
    );

    match grant_result {
        Ok(_) => {
            log::info!(
                "rlobkit-dialogs: took persistable URI permission for {} with flags=0x{:x}",
                content_uri,
                flags
            );
        }
        Err(JniError::JavaException) => {
            let detail = match env.exception_catch() {
                Err(caught) => caught.to_string(),
                Ok(()) => "Java exception (no detail available)".to_string(),
            };
            log::warn!(
                "rlobkit-dialogs: persistable URI permission unavailable for {}: {}",
                content_uri,
                detail
            );
        }
        Err(other) => {
            log::warn!(
                "rlobkit-dialogs: failed to request persistable URI permission for {}: {}",
                content_uri,
                other
            );
        }
    }
}

fn best_effort_grant_self_uri_permission(
    env: &mut Env<'_>,
    context: &JObject<'_>,
    uri: &JObject<'_>,
    flags: i32,
    content_uri: &str,
) {
    let mut rw_flags = flags & (FLAG_GRANT_READ_URI_PERMISSION | FLAG_GRANT_WRITE_URI_PERMISSION);
    if rw_flags == 0 {
        rw_flags = FLAG_GRANT_READ_URI_PERMISSION;
    }

    let package_name_obj = match env.call_method(
        context,
        jni_str!("getPackageName"),
        jni_sig!("()Ljava/lang/String;"),
        &[],
    ) {
        Ok(v) => match v.l() {
            Ok(obj) if !obj.is_null() => obj,
            _ => return,
        },
        _ => return,
    };

    let _ = env.call_method(
        context,
        jni_str!("grantUriPermission"),
        jni_sig!("(Ljava/lang/String;Landroid/net/Uri;I)V"),
        &[
            JValue::Object(&package_name_obj),
            JValue::Object(uri),
            JValue::Int(rw_flags),
        ],
    );

    log::info!(
        "rlobkit-dialogs: attempted grantUriPermission for {} flags=0x{:x}",
        content_uri,
        rw_flags
    );
}

fn build_helper_launch_intent<'a>(
    env: &mut Env<'a>,
    context: &JObject<'a>,
    target_intent: &Intent<'a>,
    request_code: i32,
) -> Result<Intent<'a>, JniError> {
    let action = JString::new(env, RLOBKIT_HELPER_LAUNCH_ACTION)?;
    let helper_intent = Intent::new(env, action)?;

    let package_name = env
        .call_method(
            context,
            jni_str!("getPackageName"),
            jni_sig!("()Ljava/lang/String;"),
            &[],
        )?
        .l()?;
    let package_name = JString::cast_local(env, package_name)?;
    let class_name = JString::new(env, RLOBKIT_HELPER_ACTIVITY_NAME)?;

    let _ = env.call_method(
        &helper_intent,
        jni_str!("setClassName"),
        jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;"),
        &[JValue::Object(&package_name), JValue::Object(&class_name)],
    )?;

    let key_target = JString::new(env, RLOBKIT_EXTRA_TARGET_INTENT)?;
    let _ = env.call_method(
        &helper_intent,
        jni_str!("putExtra"),
        jni_sig!("(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;"),
        &[JValue::Object(&key_target), JValue::Object(target_intent)],
    )?;

    let grant_flags = FLAG_GRANT_READ_URI_PERMISSION
        | FLAG_GRANT_WRITE_URI_PERMISSION
        | FLAG_GRANT_PERSISTABLE_URI_PERMISSION
        | FLAG_GRANT_PREFIX_URI_PERMISSION;
    let _ = env.call_method(
        &helper_intent,
        jni_str!("addFlags"),
        jni_sig!("(I)Landroid/content/Intent;"),
        &[JValue::Int(grant_flags)],
    )?;

    let key_request_code = JString::new(env, RLOBKIT_EXTRA_REQUEST_CODE)?;
    let _ = env.call_method(
        &helper_intent,
        jni_str!("putExtra"),
        jni_sig!("(Ljava/lang/String;I)Landroid/content/Intent;"),
        &[JValue::Object(&key_request_code), JValue::Int(request_code)],
    )?;

    Ok(helper_intent)
}

fn start_helper_activity_for_result(
    target_intent: Global<Intent<'static>>,
    request_code: i32,
) -> Result<(), RlobKitError> {
    log::info!("rlobkit-dialogs: launching helper activity request_code={request_code}");
    with_android_env(move |env| {
        let context =
            current_context(env).map_err(|e| annotate_jni_error(env, "current_context", e))?;

        let target_ref = env
            .new_local_ref(target_intent.as_obj())
            .map_err(|e| annotate_jni_error(env, "new_local_ref(target)", e))?;
        let target_intent = Intent::cast_local(env, target_ref)
            .map_err(|e| annotate_jni_error(env, "cast_local(target)", e))?;
        let helper_intent = build_helper_launch_intent(env, &context, &target_intent, request_code)
            .map_err(|e| annotate_jni_error(env, "build_helper_launch_intent", e))?;

        let _ = env
            .call_method(
                &helper_intent,
                jni_str!("addFlags"),
                jni_sig!("(I)Landroid/content/Intent;"),
                &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)],
            )
            .map_err(|e| annotate_jni_error(env, "Intent.addFlags", e))?;

        let _ = env
            .call_method(
                &context,
                jni_str!("startActivity"),
                jni_sig!("(Landroid/content/Intent;)V"),
                &[JValue::Object(&helper_intent)],
            )
            .map_err(|e| annotate_jni_error(env, "Context.startActivity", e))?;
        Ok(())
    })
}

fn to_uri_string(env: &mut Env<'_>, uri: &Uri<'_>) -> Result<Option<String>, JniError> {
    if uri.is_null() {
        return Ok(None);
    }
    let text = uri.to_java_string(env)?;
    text.try_to_string(env).map(Some)
}

fn take_persistable_uri_permission(uri: &str, grant_flags: i32) -> Result<(), RlobKitError> {
    with_android_env(|env| {
        if (grant_flags & FLAG_GRANT_PERSISTABLE_URI_PERMISSION) == 0 {
            log::info!(
                "rlobkit-dialogs: skip persist URI permission for {} (flags=0x{:x})",
                uri,
                grant_flags
            );
            return Ok(());
        }

        let context = current_context(env)?;
        let resolver = env
            .call_method(
                &context,
                jni_str!("getContentResolver"),
                jni_sig!("()Landroid/content/ContentResolver;"),
                &[],
            )?
            .l()?;
        let resolver = ContentResolver::cast_local(env, resolver)?;

        let uri_text = uri.to_string();
        let uri = JString::new(env, uri)?;
        let uri = Uri::parse(env, uri)?;

        let mut flags =
            grant_flags & (FLAG_GRANT_READ_URI_PERMISSION | FLAG_GRANT_WRITE_URI_PERMISSION);
        if flags == 0 {
            flags = FLAG_GRANT_READ_URI_PERMISSION;
        }

        log::info!(
            "rlobkit-dialogs: takePersistableUriPermission uri={} flags=0x{:x} rw=0x{:x}",
            uri_text,
            grant_flags,
            flags
        );
        resolver.take_persistable_uri_permission(env, uri, flags)
    })
}

fn prepare_open_document_intent(
    opts: &OpenFileOptions,
    allow_multiple: bool,
) -> Result<Global<Intent<'static>>, RlobKitError> {
    with_android_env(|env| {
        let action = JString::new(env, ACTION_OPEN_DOCUMENT)?;
        let mut intent = Intent::new(env, action)?;

        let openable = JString::new(env, CATEGORY_OPENABLE)?;
        intent = intent.add_category(env, openable)?;

        let mime = opts.file_type.mime_types();
        let mime = if mime.is_empty() {
            "*/*"
        } else if mime.len() == 1 {
            mime[0]
        } else {
            "*/*"
        };

        let mime = JString::new(env, mime)?;
        intent = intent.set_type(env, mime)?;
        put_mime_filters(env, &intent, &opts.file_type.mime_types())?;

        let request_flags = FLAG_GRANT_READ_URI_PERMISSION
            | FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            | FLAG_GRANT_PREFIX_URI_PERMISSION;
        let _ = env.call_method(
            &intent,
            jni_str!("addFlags"),
            jni_sig!("(I)Landroid/content/Intent;"),
            &[JValue::Int(request_flags)],
        )?;

        if allow_multiple {
            let key = JString::new(env, EXTRA_ALLOW_MULTIPLE)?;
            let _ = intent.put_extra_bool(env, key, true)?;
        }

        if let Some(title) = &opts.title {
            let key = JString::new(env, EXTRA_TITLE)?;
            let value = JString::new(env, title.as_str())?;
            let _ = intent.put_extra_string(env, key, value)?;
        }

        env.new_cast_global_ref::<Intent>(&intent)
    })
}

fn prepare_open_directory_intent(
    opts: &OpenDirectoryOptions,
) -> Result<Global<Intent<'static>>, RlobKitError> {
    with_android_env(|env| {
        let action = JString::new(env, ACTION_OPEN_DOCUMENT_TREE)?;
        let intent = Intent::new(env, action)?;

        let request_flags = FLAG_GRANT_READ_URI_PERMISSION
            | FLAG_GRANT_WRITE_URI_PERMISSION
            | FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            | FLAG_GRANT_PREFIX_URI_PERMISSION;
        let _ = env.call_method(
            &intent,
            jni_str!("addFlags"),
            jni_sig!("(I)Landroid/content/Intent;"),
            &[JValue::Int(request_flags)],
        )?;

        if let Some(title) = &opts.title {
            let key = JString::new(env, EXTRA_TITLE)?;
            let value = JString::new(env, title.as_str())?;
            let _ = intent.put_extra_string(env, key, value)?;
        }

        env.new_cast_global_ref::<Intent>(&intent)
    })
}

fn prepare_create_document_intent(
    opts: &SaveFileOptions,
) -> Result<Global<Intent<'static>>, RlobKitError> {
    with_android_env(|env| {
        let action = JString::new(env, ACTION_CREATE_DOCUMENT)?;
        let mut intent = Intent::new(env, action)?;

        let openable = JString::new(env, CATEGORY_OPENABLE)?;
        intent = intent.add_category(env, openable)?;

        let mime = JString::new(env, mime_from_extension(opts.extension.as_deref()))?;
        intent = intent.set_type(env, mime)?;

        let request_flags = FLAG_GRANT_READ_URI_PERMISSION
            | FLAG_GRANT_WRITE_URI_PERMISSION
            | FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            | FLAG_GRANT_PREFIX_URI_PERMISSION;
        let _ = env.call_method(
            &intent,
            jni_str!("addFlags"),
            jni_sig!("(I)Landroid/content/Intent;"),
            &[JValue::Int(request_flags)],
        )?;

        let suggested_name = match (&opts.suggested_name, &opts.extension) {
            (Some(name), Some(ext)) if !name.ends_with(ext.trim_start_matches('.')) => {
                format!("{}.{}", name, ext.trim_start_matches('.'))
            }
            (Some(name), _) => name.clone(),
            (None, Some(ext)) => format!("untitled.{}", ext.trim_start_matches('.')),
            (None, None) => "untitled".to_string(),
        };

        let key = JString::new(env, EXTRA_TITLE)?;
        let value = JString::new(env, suggested_name.as_str())?;
        let _ = intent.put_extra_string(env, key, value)?;

        env.new_cast_global_ref::<Intent>(&intent)
    })
}

fn launch_and_wait(
    intent: Global<Intent<'static>>,
    request_code: i32,
) -> Result<ActivityResult, RlobKitError> {
    if !helper_activity_available() {
        log::warn!("rlobkit-dialogs: helper activity precheck failed; attempting launch anyway");
    }

    begin_request(request_code)?;
    let launch_result = start_helper_activity_for_result(intent, request_code);

    if let Err(error) = launch_result {
        cancel_request();
        return Err(error);
    }
    wait_for_result(request_code)
}

pub fn read_file_to_path(source: &PlatformFile, dest_path: &Path) -> Result<(), RlobKitError> {
    let uri = source.uri().ok_or_else(|| {
        RlobKitError::UnsupportedOperation("Android source is not a content URI".into())
    })?;

    let fd = with_android_env(|env| -> Result<i32, JniError> {
        let context = current_context(env).map_err(|e| annotate_jni_error(env, "read.context", e))?;

        let resolver = env
            .call_method(
                &context,
                jni_str!("getContentResolver"),
                jni_sig!("()Landroid/content/ContentResolver;"),
                &[],
            )
            .map_err(|e| annotate_jni_error(env, "read.getContentResolver", e))?
            .l()
            .map_err(|e| annotate_jni_error(env, "read.getContentResolver.as_l", e))?;

        let juri_class: jni::objects::JClass<'_> = env
            .find_class(jni_str!("android/net/Uri"))
            .map_err(|e| annotate_jni_error(env, "read.findClass(Uri)", e))?;
        let juri_text = JString::new(env, uri)
            .map_err(|e| annotate_jni_error(env, "read.newString(uri)", e))?;
        let juri = env
            .call_static_method(
                juri_class,
                jni_str!("parse"),
                jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
                &[JValue::Object(&juri_text.into())],
            )
            .map_err(|e| annotate_jni_error(env, "read.Uri.parse", e))?
            .l()
            .map_err(|e| annotate_jni_error(env, "read.Uri.parse.as_l", e))?;

        best_effort_grant_self_uri_permission(
            env,
            &context,
            &juri,
            FLAG_GRANT_READ_URI_PERMISSION,
            uri,
        );
        best_effort_take_persistable_uri_permission(
            env,
            &resolver,
            &juri,
            FLAG_GRANT_READ_URI_PERMISSION,
            uri,
        );

        let mode = JString::new(env, "r")
            .map_err(|e| annotate_jni_error(env, "read.newString(mode)", e))?;
        let mode_obj: JObject<'_> = mode.into();

        let pfd = match env.call_method(
            &resolver,
            jni_str!("openFileDescriptor"),
            jni_sig!("(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;"),
            &[JValue::Object(&juri), JValue::Object(&mode_obj)],
        ) {
            Ok(value) => value
                .l()
                .map_err(|e| annotate_jni_error(env, "read.openFileDescriptor.as_l", e))?,
            Err(JniError::JavaException) => {
                let _ = env.exception_catch();
                return Err(JniError::MethodNotFound {
                    name: "openFileDescriptor".into(),
                    sig: "Java exception".into(),
                });
            }
            Err(other) => return Err(annotate_jni_error(env, "read.openFileDescriptor", other)),
        };

        if pfd.is_null() {
            return Err(JniError::MethodNotFound {
                name: "openFileDescriptor".into(),
                sig: "returned null".into(),
            });
        }

        let fd = env
            .call_method(&pfd, jni_str!("detachFd"), jni_sig!("()I"), &[])
            .map_err(|e| annotate_jni_error(env, "read.detachFd", e))?
            .i()
            .map_err(|e| annotate_jni_error(env, "read.detachFd.as_i", e))?;

        let _ = env.delete_local_ref(pfd);
        Ok(fd)
    })?;

    if fd < 0 {
        return Err(RlobKitError::UnsupportedOperation(format!(
            "Failed to obtain readable file descriptor (fd={})",
            fd
        )));
    }

    let mut source_file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut dest_file = std::fs::File::create(dest_path)?;
    std::io::copy(&mut source_file, &mut dest_file).map_err(RlobKitError::from)?;

    Ok(())
}

pub fn write_file_from_path(target: &PlatformFile, source_path: &Path) -> Result<(), RlobKitError> {
    let uri = target.uri().ok_or_else(|| {
        RlobKitError::UnsupportedOperation("Android target is not a content URI".into())
    })?;

    if let Some(fd) = take_writable_fd_for_uri(uri) {
        let mut source = std::fs::File::open(source_path)?;
        let mut sink = unsafe { std::fs::File::from_raw_fd(fd) };
        std::io::copy(&mut source, &mut sink)?;
        use std::io::Write;
        sink.flush()?;
        return Ok(());
    }

    with_android_env(|env| {
        let context =
            current_context(env).map_err(|e| annotate_jni_error(env, "write.context", e))?;
        let resolver = env
            .call_method(
                &context,
                jni_str!("getContentResolver"),
                jni_sig!("()Landroid/content/ContentResolver;"),
                &[],
            )
            .map_err(|e| annotate_jni_error(env, "write.getContentResolver", e))?
            .l()
            .map_err(|e| annotate_jni_error(env, "write.getContentResolver.as_l", e))?;

        let juri_class: jni::objects::JClass<'_> = env
            .find_class(jni_str!("android/net/Uri"))
            .map_err(|e| annotate_jni_error(env, "write.findClass(Uri)", e))?;
        let juri_text = JString::new(env, uri)
            .map_err(|e| annotate_jni_error(env, "write.newString(uri)", e))?;
        let juri = env
            .call_static_method(
                juri_class,
                jni_str!("parse"),
                jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
                &[JValue::Object(&juri_text.into())],
            )
            .map_err(|e| annotate_jni_error(env, "write.Uri.parse", e))?
            .l()
            .map_err(|e| annotate_jni_error(env, "write.Uri.parse.as_l", e))?;

        best_effort_grant_self_uri_permission(
            env,
            &context,
            &juri,
            FLAG_GRANT_READ_URI_PERMISSION | FLAG_GRANT_WRITE_URI_PERMISSION,
            uri,
        );
        best_effort_take_persistable_uri_permission(
            env,
            &resolver,
            &juri,
            FLAG_GRANT_READ_URI_PERMISSION | FLAG_GRANT_WRITE_URI_PERMISSION,
            uri,
        );

        let out_stream = {
            let mode = JString::new(env, "wt")
                .map_err(|e| annotate_jni_error(env, "write.newString(mode)", e))?;
            let mode_obj: JObject<'_> = mode.into();
            match env.call_method(
                &resolver,
                jni_str!("openOutputStream"),
                jni_sig!("(Landroid/net/Uri;Ljava/lang/String;)Ljava/io/OutputStream;"),
                &[JValue::Object(&juri), JValue::Object(&mode_obj)],
            ) {
                Ok(value) => value
                    .l()
                    .map_err(|e| annotate_jni_error(env, "write.openOutputStream(wt).as_l", e))?,
                Err(JniError::JavaException) => {
                    let _ = env.exception_catch();
                    env.call_method(
                        &resolver,
                        jni_str!("openOutputStream"),
                        jni_sig!("(Landroid/net/Uri;)Ljava/io/OutputStream;"),
                        &[JValue::Object(&juri)],
                    )
                    .map_err(|e| annotate_jni_error(env, "write.openOutputStream(default)", e))?
                    .l()
                    .map_err(|e| {
                        annotate_jni_error(env, "write.openOutputStream(default).as_l", e)
                    })?
                }
                Err(other) => {
                    return Err(annotate_jni_error(env, "write.openOutputStream(wt)", other));
                }
            }
        };

        if out_stream.is_null() {
            return Err(JniError::MethodNotFound {
                name: "openOutputStream".into(),
                sig: "returned null".into(),
            });
        }

        let mut file = std::fs::File::open(source_path).map_err(|e| JniError::MethodNotFound {
            name: "File.open".into(),
            sig: format!("{}: {e}", source_path.display()),
        })?;
        let mut buf = [0u8; 64 * 1024];
        loop {
            use std::io::Read;
            let n = file.read(&mut buf).map_err(|e| JniError::MethodNotFound {
                name: "File.read".into(),
                sig: e.to_string(),
            })?;
            if n == 0 {
                break;
            }

            let jarr: JByteArray<'_> = env
                .new_byte_array(n)
                .map_err(|e| annotate_jni_error(env, "write.new_byte_array", e))?;
            let tmp_i8: &[i8] = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const i8, n) };
            jarr.set_region(env, 0, tmp_i8)
                .map_err(|e| annotate_jni_error(env, "write.set_byte_array_region", e))?;
            let jarr_obj: JObject<'_> = jarr.into();
            env.call_method(
                &out_stream,
                jni_str!("write"),
                jni_sig!("([B)V"),
                &[JValue::Object(&jarr_obj)],
            )
            .map_err(|e| annotate_jni_error(env, "write.OutputStream.write", e))?;
            let _ = env.delete_local_ref(jarr_obj);
        }

        env.call_method(&out_stream, jni_str!("flush"), jni_sig!("()V"), &[])
            .map_err(|e| annotate_jni_error(env, "write.OutputStream.flush", e))?;
        env.call_method(&out_stream, jni_str!("close"), jni_sig!("()V"), &[])
            .map_err(|e| annotate_jni_error(env, "write.OutputStream.close", e))?;
        Ok(())
    })
}

fn uris_from_result(result: ActivityResult, allow_multiple: bool) -> Vec<String> {
    if allow_multiple {
        if result.clip_uris.is_empty() {
            result.data_uri.into_iter().collect()
        } else {
            result.clip_uris
        }
    } else {
        result
            .data_uri
            .into_iter()
            .chain(result.clip_uris.into_iter().take(1))
            .collect()
    }
}

pub async fn open_file_picker(
    opts: OpenFileOptions,
) -> Result<Option<Vec<PlatformFile>>, RlobKitError> {
    let allow_multiple = matches!(opts.mode, RlobKitMode::Multiple { .. });
    let request_code = if allow_multiple {
        REQUEST_OPEN_MULTI
    } else {
        REQUEST_OPEN_SINGLE
    };

    let intent = prepare_open_document_intent(&opts, allow_multiple)?;
    let result = launch_and_wait(intent, request_code)?;
    if result.result_code != RESULT_OK {
        return Ok(None);
    }

    let grant_flags = result.grant_flags;
    let mut uris = uris_from_result(result, allow_multiple);
    if let RlobKitMode::Multiple { limit: Some(limit) } = opts.mode {
        uris.truncate(limit);
    }

    if uris.is_empty() {
        return Ok(None);
    }

    let mut files = Vec::with_capacity(uris.len());
    for uri in uris {
        if let Err(err) = take_persistable_uri_permission(&uri, grant_flags) {
            log::warn!(
                "rlobkit-dialogs: failed to persist URI permission for {}: {}",
                uri,
                err
            );
        }
        files.push(PlatformFile::from_uri(uri));
    }

    Ok(Some(files))
}

pub async fn open_directory_picker(
    opts: OpenDirectoryOptions,
) -> Result<Option<PlatformDirectory>, RlobKitError> {
    let intent = prepare_open_directory_intent(&opts)?;
    let result = launch_and_wait(intent, REQUEST_OPEN_DIRECTORY)?;
    if result.result_code != RESULT_OK {
        return Ok(None);
    }

    let uri = result
        .data_uri
        .or_else(|| result.clip_uris.into_iter().next())
        .ok_or_else(|| RlobKitError::InvalidUri("No directory URI returned".into()))?;

    if let Err(err) = take_persistable_uri_permission(&uri, result.grant_flags) {
        log::warn!(
            "rlobkit-dialogs: failed to persist directory URI permission for {}: {}",
            uri,
            err
        );
    }
    Ok(Some(PlatformDirectory::new(uri)))
}

pub async fn open_file_saver(opts: SaveFileOptions) -> Result<Option<PlatformFile>, RlobKitError> {
    let intent = prepare_create_document_intent(&opts)?;
    let result = launch_and_wait(intent, REQUEST_CREATE_DOCUMENT)?;
    if result.result_code != RESULT_OK {
        if let Some(fd) = result.open_fd {
            close_raw_fd(fd);
        }
        return Ok(None);
    }

    let ActivityResult {
        data_uri,
        clip_uris,
        grant_flags,
        open_fd,
        ..
    } = result;

    let uri = data_uri
        .or_else(|| clip_uris.into_iter().next())
        .ok_or_else(|| RlobKitError::InvalidUri("No save URI returned".into()))?;

    if let Some(fd) = open_fd {
        stash_writable_fd_for_uri(&uri, fd);
    }

    if let Err(err) = take_persistable_uri_permission(&uri, grant_flags) {
        log::warn!(
            "rlobkit-dialogs: failed to persist save URI permission for {}: {}",
            uri,
            err
        );
    }
    Ok(Some(PlatformFile::from_uri(uri)))
}

fn on_activity_result_internal(
    request_code: i32,
    result_code: i32,
    data_uri: Option<String>,
    clip_uris: Vec<String>,
    grant_flags: i32,
    open_fd: Option<i32>,
) {
    log::info!(
        "rlobkit-dialogs: on_activity_result request={request_code} result={result_code} data_uri={} clip_count={} flags=0x{:x} open_fd={}",
        data_uri.is_some(),
        clip_uris.len(),
        grant_flags,
        open_fd.is_some(),
    );
    let (lock, cvar) = pending_state();
    if let Ok(mut guard) = lock.lock() {
        if guard.request_code != Some(request_code) {
            log::warn!(
                "rlobkit-dialogs: ignoring result for request={request_code}, pending={:?}",
                guard.request_code
            );
            return;
        }

        guard.result = Some(ActivityResult {
            result_code,
            data_uri,
            clip_uris,
            grant_flags,
            open_fd,
        });
        cvar.notify_all();
    }
}

pub fn on_activity_result(
    request_code: i32,
    result_code: i32,
    data_uri: Option<String>,
    clip_uris: Vec<String>,
    grant_flags: i32,
    open_fd: Option<i32>,
) {
    on_activity_result_internal(
        request_code,
        result_code,
        data_uri,
        clip_uris,
        grant_flags,
        open_fd,
    );
}

pub fn on_activity_result_from_intent(
    env: &mut Env<'_>,
    request_code: i32,
    result_code: i32,
    data: JObject<'_>,
) -> Result<(), RlobKitError> {
    log::info!(
        "rlobkit-dialogs: on_activity_result_from_intent request={request_code} result={result_code} null_data={}",
        data.is_null()
    );
    if data.is_null() {
        on_activity_result_internal(request_code, result_code, None, Vec::new(), 0, None);
        return Ok(());
    }

    let intent = Intent::cast_local(env, data).map_err(map_jni_error)?;
    let grant_flags = intent.get_flags(env).map_err(map_jni_error)?;
    let open_fd_key = JString::new(env, EXTRA_OPEN_FD).map_err(map_jni_error)?;
    let open_fd_key_obj: JObject<'_> = open_fd_key.into();
    let open_fd = env
        .call_method(
            &intent,
            jni_str!("getIntExtra"),
            jni_sig!("(Ljava/lang/String;I)I"),
            &[JValue::Object(&open_fd_key_obj), JValue::Int(-1)],
        )
        .ok()
        .and_then(|v| v.i().ok())
        .filter(|v| *v >= 0);
    log::info!(
        "rlobkit-dialogs: result intent flags=0x{:x} request={request_code} open_fd={}",
        grant_flags,
        open_fd.is_some()
    );

    let data_uri = intent
        .get_data(env)
        .map_err(map_jni_error)
        .and_then(|uri| to_uri_string(env, &uri).map_err(map_jni_error))?;

    let mut clip_uris = Vec::new();
    let clip_data = intent.get_clip_data(env).map_err(map_jni_error)?;
    if !clip_data.is_null() {
        let count = clip_data.get_item_count(env).map_err(map_jni_error)?;
        for idx in 0..count {
            let item = clip_data.get_item_at(env, idx).map_err(map_jni_error)?;
            if item.is_null() {
                continue;
            }
            let uri = item.get_uri(env).map_err(map_jni_error)?;
            if let Some(uri) = to_uri_string(env, &uri).map_err(map_jni_error)? {
                clip_uris.push(uri);
            }
        }
    }

    on_activity_result_internal(
        request_code,
        result_code,
        data_uri,
        clip_uris,
        grant_flags,
        open_fd,
    );

    Ok(())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_rust_rlobkit_RlobKitPickerActivity_nativeOnActivityResult(
    mut env: EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
    request_code: jni::sys::jint,
    result_code: jni::sys::jint,
    data: JObject<'_>,
) {
    let _ = env.with_env(|env| -> jni::errors::Result<()> {
        on_activity_result_from_intent(env, request_code, result_code, data).map_err(|error| {
            JniError::MethodNotFound {
                name: "nativeOnActivityResult".into(),
                sig: error.to_string(),
            }
            .into()
        })
    });
}
