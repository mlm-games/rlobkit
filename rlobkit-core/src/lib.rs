//! rlobkit-core: PlatformFile, PlatformDirectory, and common file operations.

pub mod error;
pub mod paths;

pub use error::RlobKitError;

use bytes::Bytes;
use std::path::{Path, PathBuf};

#[cfg(feature = "tokio-runtime")]
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformFile {
    inner: PlatformFileInner,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlatformFileInner {
    Path(PathBuf),
    #[cfg(target_os = "android")]
    Uri(String),
    #[cfg(target_arch = "wasm32")]
    Blob {
        name: String,
        data: Bytes,
    },
}

impl PlatformFile {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self {
            inner: PlatformFileInner::Path(path.into()),
        }
    }

    #[cfg(target_os = "android")]
    pub fn from_uri(uri: impl Into<String>) -> Self {
        Self {
            inner: PlatformFileInner::Uri(uri.into()),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn from_blob(name: impl Into<String>, data: Bytes) -> Self {
        Self {
            inner: PlatformFileInner::Blob {
                name: name.into(),
                data,
            },
        }
    }

    pub fn name(&self) -> Option<String> {
        match &self.inner {
            PlatformFileInner::Path(p) => p.file_name()?.to_str().map(String::from),
            #[cfg(target_os = "android")]
            PlatformFileInner::Uri(u) => u.split('/').last().map(String::from),
            #[cfg(target_arch = "wasm32")]
            PlatformFileInner::Blob { name, .. } => Some(name.clone()),
        }
    }

    pub fn extension(&self) -> Option<String> {
        match &self.inner {
            PlatformFileInner::Path(p) => p.extension()?.to_str().map(String::from),
            #[cfg(target_os = "android")]
            PlatformFileInner::Uri(_) => self.name()?.rsplit('.').next().map(String::from),
            #[cfg(target_arch = "wasm32")]
            PlatformFileInner::Blob { .. } => self.name()?.rsplit('.').next().map(String::from),
        }
    }

    pub fn mime_type(&self) -> Option<String> {
        let ext = self.extension()?;
        Some(
            mime_guess::from_ext(&ext)
                .first_or_octet_stream()
                .to_string(),
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_bytes(&self) -> Result<Bytes, RlobKitError> {
        match &self.inner {
            PlatformFileInner::Path(p) => Ok(Bytes::from(std::fs::read(p)?)),
            #[cfg(target_os = "android")]
            PlatformFileInner::Uri(_) => Err(RlobKitError::UnsupportedOperation(
                "Use read_bytes_async on Android".into(),
            )),
        }
    }

    #[cfg(feature = "tokio-runtime")]
    pub async fn read_bytes_async(&self) -> Result<Bytes, RlobKitError> {
        match &self.inner {
            PlatformFileInner::Path(p) => {
                let mut file = tokio::fs::File::open(p).await?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).await?;
                Ok(Bytes::from(buffer))
            }
            #[cfg(target_arch = "wasm32")]
            PlatformFileInner::Blob { data, .. } => Ok(data.clone()),
            #[cfg(target_os = "android")]
            PlatformFileInner::Uri(_) => Err(RlobKitError::UnsupportedOperation(
                "Android URI reading requires RlobKit::read_file_to_path (then read the local file)".into(),
            )),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn write_bytes(&self, data: &[u8]) -> Result<(), RlobKitError> {
        match &self.inner {
            PlatformFileInner::Path(p) => Ok(std::fs::write(p, data)?),
            #[cfg(target_os = "android")]
            PlatformFileInner::Uri(_) => Err(RlobKitError::UnsupportedOperation(
                "Use write_bytes_async on Android".into(),
            )),
        }
    }

    pub fn write_string(&self, s: &str) -> Result<(), RlobKitError> {
        self.write_bytes(s.as_bytes())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn size(&self) -> Result<u64, RlobKitError> {
        match &self.inner {
            PlatformFileInner::Path(p) => Ok(std::fs::metadata(p)?.len()),
            #[cfg(target_os = "android")]
            PlatformFileInner::Uri(_) => Err(RlobKitError::UnsupportedOperation(
                "Size is not available for Android URI".into(),
            )),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn path(&self) -> Option<&Path> {
        match &self.inner {
            PlatformFileInner::Path(p) => Some(p),
            #[cfg(target_os = "android")]
            PlatformFileInner::Uri(_) => None,
        }
    }

    #[cfg(target_os = "android")]
    pub fn uri(&self) -> Option<&str> {
        match &self.inner {
            PlatformFileInner::Uri(uri) => Some(uri.as_str()),
            PlatformFileInner::Path(_) => None,
        }
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
        PlatformFile::from_path(self.path.join(name))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn list_files(&self) -> Result<Vec<PlatformFile>, RlobKitError> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                files.push(PlatformFile::from_path(entry.path()));
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
