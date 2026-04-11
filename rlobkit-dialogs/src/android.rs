use crate::picker::{OpenDirectoryOptions, OpenFileOptions, SaveFileOptions};
use crate::RlobKitMode;
use jni::{
    Env,
    errors::Error as JniError,
    objects::{JObject, JObjectArray, JString},
    refs::Global,
};
use jni_min_helper::{DynamicProxy, android_context, jni_with_env};
use rlobkit_core::{PlatformDirectory, PlatformFile, RlobKitError};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

const REQUEST_OPEN_SINGLE: i32 = 41001;
const REQUEST_OPEN_MULTI: i32 = 41002;
const REQUEST_OPEN_DIRECTORY: i32 = 41003;
const REQUEST_CREATE_DOCUMENT: i32 = 41004;

const RESULT_OK: i32 = -1;
const WAIT_TIMEOUT: Duration = Duration::from_secs(120);

const FLAG_GRANT_READ_URI_PERMISSION: i32 = 1;
const FLAG_GRANT_WRITE_URI_PERMISSION: i32 = 2;
const FLAG_GRANT_PERSISTABLE_URI_PERMISSION: i32 = 64;

const EXTRA_ALLOW_MULTIPLE: &str = "android.intent.extra.ALLOW_MULTIPLE";
const EXTRA_MIME_TYPES: &str = "android.intent.extra.MIME_TYPES";
const EXTRA_TITLE: &str = "android.intent.extra.TITLE";

const ACTION_OPEN_DOCUMENT: &str = "android.intent.action.OPEN_DOCUMENT";
const ACTION_OPEN_DOCUMENT_TREE: &str = "android.intent.action.OPEN_DOCUMENT_TREE";
const ACTION_CREATE_DOCUMENT: &str = "android.intent.action.CREATE_DOCUMENT";

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
}

fn pending_state() -> &'static (Mutex<PendingRequest>, Condvar) {
    static STATE: OnceLock<(Mutex<PendingRequest>, Condvar)> = OnceLock::new();
    STATE.get_or_init(|| (Mutex::new(PendingRequest::default()), Condvar::new()))
}

fn map_jni_error(error: JniError) -> RlobKitError {
    RlobKitError::UnsupportedOperation(format!("Android JNI error: {error}"))
}

fn with_android_env<T>(
    f: impl FnOnce(&mut Env<'_>) -> Result<T, JniError>,
) -> Result<T, RlobKitError> {
    jni_with_env(f).map_err(map_jni_error)
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
    Ok(())
}

fn cancel_request() {
    let (lock, cvar) = pending_state();
    if let Ok(mut guard) = lock.lock() {
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
            return Ok(result);
        }

        let now = Instant::now();
        if now >= deadline {
            guard.request_code = None;
            return Err(RlobKitError::UnsupportedOperation(
                "Timed out waiting for Android activity result; wire on_activity_result callback"
                    .into(),
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
    Activity => "android.app.Activity",
    type_map = {
        Intent => "android.content.Intent",
        ContentResolver => "android.content.ContentResolver",
    },
    methods {
        fn start_activity_for_result {
            name = "startActivityForResult",
            sig = (intent: Intent, request_code: jint) -> (),
        },
        fn get_content_resolver() -> ContentResolver,
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

fn current_activity<'a>(env: &mut Env<'a>) -> Result<Activity<'a>, JniError> {
    let local = env.new_local_ref(android_context())?;
    Activity::cast_local(env, local)
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

fn put_mime_filters(env: &mut Env<'_>, intent: &Intent<'_>, mimes: &[&str]) -> Result<(), JniError> {
    if mimes.is_empty() || (mimes.len() == 1 && mimes[0] == "*/*") {
        return Ok(());
    }

    let key = JString::new(env, EXTRA_MIME_TYPES)?;
    let values = string_array(env, mimes)?;
    let _ = intent.put_extra_string_array(env, key, values)?;
    Ok(())
}

fn start_intent_for_result(
    intent: Global<Intent<'static>>,
    request_code: i32,
) -> Result<(), RlobKitError> {
    with_android_env(|env| {
        let activity = current_activity(env)?;
        let activity = env.new_global_ref(activity)?;

        let posted = DynamicProxy::post_to_main_looper(move |env| {
            let activity_ref = env.new_local_ref(activity.as_obj())?;
            let activity = Activity::cast_local(env, activity_ref)?;

            let intent_ref = env.new_local_ref(intent.as_obj())?;
            let intent = Intent::cast_local(env, intent_ref)?;

            activity.start_activity_for_result(env, intent, request_code)
        })?;

        if posted {
            Ok(())
        } else {
            Err(JniError::MethodNotFound {
                name: "post_to_main_looper".into(),
                sig: "startActivityForResult".into(),
            })
        }
    })
}

fn to_uri_string(env: &mut Env<'_>, uri: &Uri<'_>) -> Result<Option<String>, JniError> {
    if uri.is_null() {
        return Ok(None);
    }
    let text = uri.to_java_string(env)?;
    text.try_to_string(env).map(Some)
}

fn take_persistable_uri_permission(uri: &str) -> Result<(), RlobKitError> {
    with_android_env(|env| {
        let activity = current_activity(env)?;
        let resolver = activity.get_content_resolver(env)?;

        let uri = JString::new(env, uri)?;
        let uri = Uri::parse(env, uri)?;

        let flags = FLAG_GRANT_READ_URI_PERMISSION
            | FLAG_GRANT_WRITE_URI_PERMISSION
            | FLAG_GRANT_PERSISTABLE_URI_PERMISSION;

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

        let mime = JString::new(env, "*/*")?;
        intent = intent.set_type(env, mime)?;

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
    begin_request(request_code)?;
    if let Err(error) = start_intent_for_result(intent, request_code) {
        cancel_request();
        return Err(error);
    }
    wait_for_result(request_code)
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

    let mut uris = uris_from_result(result, allow_multiple);
    if let RlobKitMode::Multiple { limit: Some(limit) } = opts.mode {
        uris.truncate(limit);
    }

    if uris.is_empty() {
        return Ok(None);
    }

    let mut files = Vec::with_capacity(uris.len());
    for uri in uris {
        let _ = take_persistable_uri_permission(&uri);
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

    let _ = take_persistable_uri_permission(&uri);
    Ok(Some(PlatformDirectory::new(uri)))
}

pub async fn open_file_saver(opts: SaveFileOptions) -> Result<Option<PlatformFile>, RlobKitError> {
    let intent = prepare_create_document_intent(&opts)?;
    let result = launch_and_wait(intent, REQUEST_CREATE_DOCUMENT)?;
    if result.result_code != RESULT_OK {
        return Ok(None);
    }

    let uri = result
        .data_uri
        .or_else(|| result.clip_uris.into_iter().next())
        .ok_or_else(|| RlobKitError::InvalidUri("No save URI returned".into()))?;

    let _ = take_persistable_uri_permission(&uri);
    Ok(Some(PlatformFile::from_uri(uri)))
}

pub fn on_activity_result(
    request_code: i32,
    result_code: i32,
    data_uri: Option<String>,
    clip_uris: Vec<String>,
) {
    let (lock, cvar) = pending_state();
    if let Ok(mut guard) = lock.lock() {
        if guard.request_code != Some(request_code) {
            return;
        }

        guard.result = Some(ActivityResult {
            result_code,
            data_uri,
            clip_uris,
        });
        cvar.notify_all();
    }
}

pub fn on_activity_result_from_intent(
    env: &mut Env<'_>,
    request_code: i32,
    result_code: i32,
    data: JObject<'_>,
) -> Result<(), RlobKitError> {
    if data.is_null() {
        on_activity_result(request_code, result_code, None, Vec::new());
        return Ok(());
    }

    let intent = Intent::cast_local(env, data).map_err(map_jni_error)?;

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

    on_activity_result(request_code, result_code, data_uri, clip_uris);
    Ok(())
}
