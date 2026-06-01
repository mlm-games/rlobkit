//! rlobkit-core: PlatformFile, PlatformDirectory, and common file operations.

pub mod error;
pub mod paths;

pub use error::RlobKitError;

use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[cfg(all(feature = "tokio-runtime", not(target_arch = "wasm32")))]
use tokio::io::AsyncReadExt;

/// Android-specific bytes I/O function pointer type.
///
/// Set at init time by `rlobkit-dialogs` (which has JNI access) to a function
/// that reads the content at a `content://` URI and returns the bytes, or
/// writes bytes to a `content://` URI.
pub type AndroidReadBytes = fn(&str) -> Result<Bytes, RlobKitError>;
pub type AndroidWriteBytes = fn(&str, &[u8]) -> Result<(), RlobKitError>;

static ANDROID_READ: OnceLock<AndroidReadBytes> = OnceLock::new();
static ANDROID_WRITE: OnceLock<AndroidWriteBytes> = OnceLock::new();

/// Register Android I/O implementations. Called by `rlobkit-dialogs::init()`
/// at app startup. No-op on non-Android targets (the pointers are never
/// invoked when no `uri` field is set).
pub fn set_android_io(read: AndroidReadBytes, write: AndroidWriteBytes) {
    let _ = ANDROID_READ.set(read);
    let _ = ANDROID_WRITE.set(write);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformFile {
    name: String,
    path: Option<PathBuf>,
    uri: Option<String>,
    data: Option<Bytes>,
    size: Option<u64>,
}

impl PlatformFile {
    pub fn from_path(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: Some(path.into()),
            uri: None,
            data: None,
            size: None,
        }
    }

    #[cfg(target_os = "android")]
    pub fn from_uri(name: impl Into<String>, uri: impl Into<String>, size: Option<u64>) -> Self {
        Self {
            name: name.into(),
            path: None,
            uri: Some(uri.into()),
            data: None,
            size,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn from_blob(name: impl Into<String>, data: Bytes) -> Self {
        let size = Some(data.len() as u64);
        Self {
            name: name.into(),
            path: None,
            uri: None,
            data: Some(data),
            size,
        }
    }

    /// Display name. Always populated at construction.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn extension(&self) -> Option<&str> {
        std::path::Path::new(&self.name)
            .extension()
            .and_then(|e| e.to_str())
    }

    pub fn mime_type(&self) -> Option<String> {
        let ext = self.extension()?;
        Some(
            mime_guess::from_ext(ext)
                .first_or_octet_stream()
                .to_string(),
        )
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Content URI on Android. Always returns `None` on platforms where the
    /// file wasn't sourced from a SAF picker. Haven't gated it for checks
    pub fn uri(&self) -> Option<&str> {
        self.uri.as_deref()
    }

    /// In-memory bytes on WASM. Always returns `None` on platforms where the
    /// file wasn't sourced from a blob picker. Haven't gated it for checks
    pub fn data(&self) -> Option<&Bytes> {
        self.data.as_ref()
    }

    /// Cached size. `None` if not resolved at construction (desktop, or Android
    /// pickers that didn't query the size).
    pub fn size(&self) -> Option<u64> {
        self.size
    }

    pub fn read_bytes(&self) -> Result<Bytes, RlobKitError> {
        if let Some(p) = &self.path {
            return Ok(Bytes::from(std::fs::read(p)?));
        }
        if let Some(u) = &self.uri {
            let reader = ANDROID_READ.get().ok_or_else(|| {
                RlobKitError::UnsupportedOperation(
                    "Android I/O not initialized; call rlobkit_dialogs::init()".into(),
                )
            })?;
            return reader(u);
        }
        if let Some(d) = &self.data {
            return Ok(d.clone());
        }
        Err(RlobKitError::UnsupportedOperation(
            "PlatformFile has no readable source".into(),
        ))
    }

    #[cfg(all(feature = "tokio-runtime", not(target_arch = "wasm32")))]
    pub async fn read_bytes_async(&self) -> Result<Bytes, RlobKitError> {
        if let Some(p) = &self.path {
            let mut file = tokio::fs::File::open(p).await?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).await?;
            return Ok(Bytes::from(buffer));
        }
        if let Some(u) = &self.uri {
            let reader = ANDROID_READ.get().ok_or_else(|| {
                RlobKitError::UnsupportedOperation(
                    "Android I/O not initialized; call rlobkit_dialogs::init()".into(),
                )
            })?;
            return reader(u);
        }
        Err(RlobKitError::UnsupportedOperation(
            "PlatformFile has no readable source".into(),
        ))
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn read_bytes_async(&self) -> Result<Bytes, RlobKitError> {
        self.read_bytes()
    }

    pub fn write_bytes(&self, data: &[u8]) -> Result<(), RlobKitError> {
        if let Some(p) = &self.path {
            std::fs::write(p, data)?;
            return Ok(());
        }
        if let Some(u) = &self.uri {
            let writer = ANDROID_WRITE.get().ok_or_else(|| {
                RlobKitError::UnsupportedOperation(
                    "Android I/O not initialized; call rlobkit_dialogs::init()".into(),
                )
            })?;
            return writer(u, data);
        }
        if self.data.is_some() {
            return Err(RlobKitError::UnsupportedOperation(
                "Writing to an in-memory blob is not supported".into(),
            ));
        }
        Err(RlobKitError::UnsupportedOperation(
            "PlatformFile has no writable destination".into(),
        ))
    }

    pub fn write_string(&self, s: &str) -> Result<(), RlobKitError> {
        self.write_bytes(s.as_bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformDirectory {
    path: PathBuf,
}

impl PlatformDirectory {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> Option<String> {
        self.path.file_name()?.to_str().map(String::from)
    }

    pub fn file(&self, name: &str) -> PlatformFile {
        PlatformFile::from_path(name, self.path.join(name))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn list_files(&self) -> Result<Vec<PlatformFile>, RlobKitError> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                files.push(PlatformFile::from_path(name, path));
            }
        }
        Ok(files)
    }
}

impl std::ops::Div<&str> for &PlatformDirectory {
    type Output = PlatformFile;
    fn div(self, rhs: &str) -> PlatformFile {
        self.file(rhs)
    }
}
