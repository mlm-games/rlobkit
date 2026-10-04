//! Incoming share/view intents, captured by the shared Activity.
//!
//! An intent is captured as a *record* — the content URI, name, MIME type, size
//! and grant flags — rather than as the file's bytes. A shared video can be
//! gigabytes, and the previous protocol read it whole on the Activity's main
//! thread, held a second copy in memory, then let Rust read it a third time.
//! Keeping the URI defers that to the point where the app actually wants the
//! data, and lets it read through [`PlatformFile::read_bytes`] or
//! [`PlatformFile::read_bytes_async`], which stream from a file descriptor.
//!
//! ## Consuming an intent
//!
//! [`take_pending_intent`] reads the oldest captured intent, for use before the
//! UI loop starts. [`drain_intents`] returns everything queued since the last
//! call, for intents that arrive while the app is running.

use rlobkit_core::PlatformFile;

/// Directory the captured intents are queued in, under the app's files dir.
#[cfg(target_os = "android")]
const QUEUE_DIR: &str = "pending_intents";

#[cfg(target_os = "android")]
const MAGIC: &[u8; 4] = b"RLKQ";

#[cfg(target_os = "android")]
const FORMAT_VERSION: u8 = 1;

/// Field length meaning "absent", so an optional string is not confused with an
/// empty one.
#[cfg(target_os = "android")]
const NONE: u32 = u32::MAX;

/// What the sender asked the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentAction {
    /// `ACTION_VIEW`: a single item.
    View,
    /// `ACTION_SEND`: one item, possibly with text.
    Send,
    /// `ACTION_SEND_MULTIPLE`: several items at once.
    SendMultiple,
}

impl IntentAction {
    /// Whether a sender using this action is expected to attach a type.
    pub const fn expects_mime(self) -> bool {
        matches!(self, Self::Send | Self::SendMultiple)
    }

    #[cfg(target_os = "android")]
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::View),
            1 => Some(Self::Send),
            2 => Some(Self::SendMultiple),
            _ => None,
        }
    }
}

/// The URI grants the sender attached to the intent.
///
/// These matter because they decide whether the app may keep reading after the
/// Activity that received the intent has gone away.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GrantFlags(u32);

impl GrantFlags {
    /// `FLAG_GRANT_READ_URI_PERMISSION`
    pub const READ: u32 = 0x1;
    /// `FLAG_GRANT_WRITE_URI_PERMISSION`
    pub const WRITE: u32 = 0x2;
    /// `FLAG_GRANT_PERSISTABLE_URI_PERMISSION`
    pub const PERSISTABLE: u32 = 0x40;
    /// `FLAG_GRANT_PREFIX_URI_PERMISSION`
    pub const PREFIX: u32 = 0x80;

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, flag: u32) -> bool {
        self.0 & flag == flag
    }

    /// Whether the URI may stay readable after the receiving Activity finishes.
    ///
    /// This reflects the sender's grant only. The Activity takes the persistable
    /// permission on capture, but that can still fail, so treat this as
    /// permission to *ask* for, not proof the URI will open later.
    pub const fn is_persisted(self) -> bool {
        self.contains(Self::PERSISTABLE) && self.contains(Self::READ)
    }
}

/// An intent delivered to the Activity.
///
/// The file entries stay as URIs on Android, so [`files`](Self::files) is cheap
/// to obtain and holds no file contents; read them on demand. Reading a URI needs
/// the Android I/O hooks registered — `rlobkit_dialogs::init()` does that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIntent {
    pub action: IntentAction,
    pub files: Vec<PlatformFile>,
    /// Shared text, from `ACTION_SEND` or `ACTION_SEND_MULTIPLE`.
    pub text: Option<String>,
    /// The type the sender declared. For a multiple send this is the common type,
    /// and individual files may differ; see each file's
    /// [`mime_type`](PlatformFile::mime_type).
    pub mime_type: Option<String>,
    pub grant_flags: GrantFlags,
}

impl AppIntent {
    /// The first file, for the common single-item share.
    pub fn file(&self) -> Option<&PlatformFile> {
        self.files.first()
    }

    /// The first file's contents, the usual shape for `ACTION_VIEW`.
    pub fn take_bytes(&self) -> Option<Result<Vec<u8>, rlobkit_core::RlobKitError>> {
        Some(self.file()?.read_bytes().map(|b| b.to_vec()))
    }
}

/// Read the oldest captured intent and remove it from the queue.
///
/// Call this from `android_main` **before** starting the UI loop, to see what
/// launched the app. Returns `None` when nothing is queued or the queue cannot be
/// read.
#[cfg(target_os = "android")]
pub fn take_pending_intent(data_dir: &std::path::Path) -> Option<AppIntent> {
    drain_intents_from(data_dir).into_iter().next()
}

/// Remove and return every captured intent, oldest first.
#[cfg(target_os = "android")]
pub fn drain_intents_from(data_dir: &std::path::Path) -> Vec<AppIntent> {
    let queue = data_dir.join(QUEUE_DIR);
    let Ok(entries) = std::fs::read_dir(&queue) else {
        return Vec::new();
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "bin"))
        .collect();
    // The Kotlin side names files so lexical order is capture order.
    paths.sort();

    let mut intents = Vec::with_capacity(paths.len());
    for path in paths {
        match std::fs::read(&path) {
            Ok(bytes) => match decode(&bytes) {
                Some(intent) => intents.push(intent),
                None => log::warn!(
                    "rlobkit_app_events: unusable intent record {}",
                    path.display()
                ),
            },
            Err(e) => log::warn!("rlobkit_app_events: cannot read {}: {e}", path.display()),
        }
        // Drop the record either way: a corrupt one must not wedge the queue.
        let _ = std::fs::remove_file(&path);
    }
    intents
}

static RUNTIME_QUEUE: std::sync::Mutex<Option<Vec<AppIntent>>> = std::sync::Mutex::new(None);

/// Queue an intent for [`drain_intents`], e.g. from a JNI `nativeOnNewIntent`
/// callback.
///
/// Safe to call before Rust static initialisers have run, since the mutex is
/// lazily allocated.
pub fn push_intent(intent: AppIntent) {
    let mut queue = RUNTIME_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    queue.get_or_insert_with(Vec::new).push(intent);
}

/// Drain every intent queued since the last call. Call this each frame in the
/// render loop.
pub fn drain_intents() -> Vec<AppIntent> {
    let mut queue = RUNTIME_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    queue.take().unwrap_or_default()
}

#[cfg(target_os = "android")]
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

#[cfg(target_os = "android")]
impl<'a> Reader<'a> {
    /// `None` means the record is truncated, which is distinct from a field that
    /// is legitimately absent.
    fn u32(&mut self) -> Option<u32> {
        let end = self.at.checked_add(4)?;
        let raw: [u8; 4] = (*self.bytes.get(self.at..end)?).try_into().ok()?;
        self.at = end;
        Some(u32::from_le_bytes(raw))
    }

    fn u64(&mut self) -> Option<u64> {
        let end = self.at.checked_add(8)?;
        let raw: [u8; 8] = (*self.bytes.get(self.at..end)?).try_into().ok()?;
        self.at = end;
        Some(u64::from_le_bytes(raw))
    }

    fn slice(&mut self, len: u32) -> Option<&'a [u8]> {
        let end = self.at.checked_add(len as usize)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    /// The outer `Option` is the record being intact; the inner one is the field
    /// being absent.
    fn string(&mut self) -> Option<Option<String>> {
        let len = self.u32()?;
        if len == NONE {
            return Some(None);
        }
        let raw = self.slice(len)?;
        Some(Some(String::from_utf8(raw.to_vec()).ok()?))
    }
}

#[cfg(target_os = "android")]
pub(crate) fn decode(bytes: &[u8]) -> Option<AppIntent> {
    let mut reader = Reader { bytes, at: 0 };
    if reader.slice(4)? != MAGIC {
        return None;
    }
    if reader.slice(1)?[0] != FORMAT_VERSION {
        return None;
    }
    let action = IntentAction::from_wire(reader.slice(1)?[0])?;
    let mime_type = reader.string()?;
    let text = reader.string()?;
    let grant_flags = GrantFlags::from_bits(reader.u32()?);
    let count = reader.u32()?;
    let mut files = Vec::with_capacity(count.min(64) as usize);
    for _ in 0..count {
        let uri = reader.string()??;
        let name = reader.string()??;
        let file_mime = reader.string()?;
        let size = reader.u64()?;
        files.push(PlatformFile::from_uri(
            name,
            uri,
            (size != u64::MAX).then_some(size),
            file_mime,
        ));
    }
    Some(AppIntent {
        action,
        files,
        text,
        mime_type,
        grant_flags,
    })
}
