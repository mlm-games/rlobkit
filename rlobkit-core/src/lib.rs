pub mod error;
pub mod paths;

pub use error::RlobKitError;

use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[cfg(all(feature = "tokio-runtime", not(target_arch = "wasm32")))]
use tokio::io::AsyncReadExt;

pub type AndroidReadBytes = fn(&str) -> Result<Bytes, RlobKitError>;
pub type AndroidWriteBytes = fn(&str, &[u8]) -> Result<(), RlobKitError>;

static ANDROID_READ: OnceLock<AndroidReadBytes> = OnceLock::new();
static ANDROID_WRITE: OnceLock<AndroidWriteBytes> = OnceLock::new();

pub fn set_android_io(read: AndroidReadBytes, write: AndroidWriteBytes) {
    let _ = ANDROID_READ.set(read);
    let _ = ANDROID_WRITE.set(write);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSource {
    Path(PathBuf),
    #[cfg(target_os = "android")]
    Uri(String),
    Bytes(Bytes),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformFile {
    name: String,
    source: FileSource,
    size: Option<u64>,
    mime_type: Option<String>,
}

impl PlatformFile {
    pub fn from_path(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            source: FileSource::Path(path.into()),
            size: None,
            mime_type: None,
        }
    }

    #[cfg(target_os = "android")]
    pub fn from_uri(
        name: impl Into<String>,
        uri: impl Into<String>,
        size: Option<u64>,
        mime_type: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            source: FileSource::Uri(uri.into()),
            size,
            mime_type,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn from_blob(name: impl Into<String>, data: Bytes, mime_type: Option<String>) -> Self {
        let size = Some(data.len() as u64);
        Self {
            name: name.into(),
            source: FileSource::Bytes(data),
            size,
            mime_type,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn extension(&self) -> Option<&str> {
        std::path::Path::new(&self.name)
            .extension()
            .and_then(|e| e.to_str())
    }

    pub fn mime_type(&self) -> Option<String> {
        if let Some(mime) = &self.mime_type {
            return Some(mime.clone());
        }
        let ext = self.extension()?;
        Some(
            mime_guess::from_ext(ext)
                .first_or_octet_stream()
                .to_string(),
        )
    }

    pub fn source(&self) -> &FileSource {
        &self.source
    }

    pub fn path(&self) -> Option<&Path> {
        match &self.source {
            FileSource::Path(p) => Some(p),
            _ => None,
        }
    }

    pub fn uri(&self) -> Option<&str> {
        #[cfg(target_os = "android")]
        {
            if let FileSource::Uri(u) = &self.source {
                return Some(u);
            }
        }
        None
    }

    pub fn data(&self) -> Option<&Bytes> {
        match &self.source {
            FileSource::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn size(&self) -> Option<u64> {
        self.size
    }

    pub fn read_bytes(&self) -> Result<Bytes, RlobKitError> {
        match &self.source {
            FileSource::Path(p) => Ok(Bytes::from(std::fs::read(p)?)),
            FileSource::Bytes(b) => Ok(b.clone()),
            #[cfg(target_os = "android")]
            FileSource::Uri(u) => {
                let reader = ANDROID_READ.get().ok_or_else(|| {
                    RlobKitError::UnsupportedOperation(
                        "Android I/O not initialized; call rlobkit_dialogs::init()".into(),
                    )
                })?;
                reader(u)
            }
        }
    }

    #[cfg(all(feature = "tokio-runtime", not(target_arch = "wasm32")))]
    pub async fn read_bytes_async(&self) -> Result<Bytes, RlobKitError> {
        match &self.source {
            FileSource::Path(p) => {
                let mut file = tokio::fs::File::open(p).await?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).await?;
                Ok(Bytes::from(buffer))
            }
            FileSource::Bytes(b) => Ok(b.clone()),
            #[cfg(target_os = "android")]
            FileSource::Uri(u) => {
                let uri = u.clone();
                let reader = ANDROID_READ.get().ok_or_else(|| {
                    RlobKitError::UnsupportedOperation(
                        "Android I/O not initialized; call rlobkit_dialogs::init()".into(),
                    )
                })?;
                tokio::task::spawn_blocking(move || reader(&uri))
                    .await
                    .map_err(|e| {
                        RlobKitError::UnsupportedOperation(format!("join error: {e}"))
                    })?
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn read_bytes_async(&self) -> Result<Bytes, RlobKitError> {
        self.read_bytes()
    }

    pub fn write_bytes(&self, data: &[u8]) -> Result<(), RlobKitError> {
        match &self.source {
            FileSource::Path(p) => {
                std::fs::write(p, data)?;
                Ok(())
            }
            FileSource::Bytes(_) => Err(RlobKitError::UnsupportedOperation(
                "Writing to an in-memory blob is not supported".into(),
            )),
            #[cfg(target_os = "android")]
            FileSource::Uri(u) => {
                let writer = ANDROID_WRITE.get().ok_or_else(|| {
                    RlobKitError::UnsupportedOperation(
                        "Android I/O not initialized; call rlobkit_dialogs::init()".into(),
                    )
                })?;
                writer(u, data)
            }
        }
    }

    pub fn write_string(&self, s: &str) -> Result<(), RlobKitError> {
        self.write_bytes(s.as_bytes())
    }

    pub fn builder(name: impl Into<String>) -> PlatformFileBuilder {
        PlatformFileBuilder {
            name: name.into(),
            source: None,
            size: None,
            mime_type: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlatformFileBuilder {
    name: String,
    source: Option<FileSource>,
    size: Option<u64>,
    mime_type: Option<String>,
}

impl PlatformFileBuilder {
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.source = Some(FileSource::Path(path.into()));
        self
    }

    #[cfg(target_os = "android")]
    pub fn uri(mut self, uri: impl Into<String>) -> Self {
        self.source = Some(FileSource::Uri(uri.into()));
        self
    }

    pub fn data(mut self, data: Bytes) -> Self {
        let size = Some(data.len() as u64);
        self.source = Some(FileSource::Bytes(data));
        self.size = size;
        self
    }

    pub fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn mime_type(mut self, mime: impl Into<String>) -> Self {
        self.mime_type = Some(mime.into());
        self
    }

    pub fn build(self) -> Result<PlatformFile, RlobKitError> {
        let source = self.source.ok_or_else(|| {
            RlobKitError::UnsupportedOperation(
                "PlatformFile must have a source (path, uri, or data)".into(),
            )
        })?;
        Ok(PlatformFile {
            name: self.name,
            source,
            size: self.size,
            mime_type: self.mime_type,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectorySource {
    Path(PathBuf),
    #[cfg(target_os = "android")]
    Uri(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformDirectory {
    source: DirectorySource,
}

impl PlatformDirectory {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            source: DirectorySource::Path(path.into()),
        }
    }

    #[cfg(target_os = "android")]
    pub fn from_uri(uri: impl Into<String>) -> Self {
        Self {
            source: DirectorySource::Uri(uri.into()),
        }
    }

    pub fn source(&self) -> &DirectorySource {
        &self.source
    }

    pub fn path(&self) -> Option<&Path> {
        match &self.source {
            DirectorySource::Path(p) => Some(p),
            #[cfg(target_os = "android")]
            _ => None,
        }
    }

    pub fn uri(&self) -> Option<&str> {
        #[cfg(target_os = "android")]
        {
            if let DirectorySource::Uri(u) = &self.source {
                return Some(u);
            }
        }
        None
    }

    pub fn name(&self) -> Option<String> {
        match &self.source {
            DirectorySource::Path(p) => p.file_name()?.to_str().map(String::from),
            #[cfg(target_os = "android")]
            DirectorySource::Uri(u) => {
                let trimmed = u.trim_end_matches('/');
                trimmed.rsplit('/').next().map(|s| s.to_string())
            }
        }
    }

    pub fn file(&self, name: &str) -> PlatformFile {
        match &self.source {
            DirectorySource::Path(p) => PlatformFile::from_path(name, p.join(name)),
            #[cfg(target_os = "android")]
            DirectorySource::Uri(u) => {
                let base = u.trim_end_matches('/');
                let uri = format!("{}/{}", base, name);
                PlatformFile::from_uri(name, uri, None, None)
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn list_files(&self) -> Result<Vec<PlatformFile>, RlobKitError> {
        match &self.source {
            DirectorySource::Path(p) => {
                let mut files = Vec::new();
                for entry in std::fs::read_dir(p)? {
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
            #[cfg(target_os = "android")]
            DirectorySource::Uri(_) => Err(RlobKitError::UnsupportedOperation(
                "Listing directory contents via SAF URI is not yet supported".into(),
            )),
        }
    }
}

impl std::ops::Div<&str> for &PlatformDirectory {
    type Output = PlatformFile;
    fn div(self, rhs: &str) -> PlatformFile {
        self.file(rhs)
    }
}

pub fn mime_to_extension(mime: &str) -> Option<&'static str> {
    if let Some(extensions) = mime_guess::get_mime_extensions_str(mime)
        && let Some(ext) = extensions.first()
    {
        return Some(ext);
    }
    match mime {
        "application/x-clap" => Some("clap"),
        _ => None,
    }
}
