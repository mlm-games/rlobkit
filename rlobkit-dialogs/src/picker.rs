use crate::{RlobKitMode, RlobKitType};
use rlobkit_core::{PlatformDirectory, PlatformFile, RlobKitError};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct OpenFileOptions {
    pub file_type: RlobKitType,
    pub mode: RlobKitMode,
    pub title: Option<String>,
    pub initial_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct SaveFileOptions {
    pub suggested_name: Option<String>,
    pub extension: Option<String>,
    pub title: Option<String>,
    pub initial_directory: Option<PathBuf>,
    pub file_type: Option<RlobKitType>,
    #[cfg(target_arch = "wasm32")]
    pub data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct OpenDirectoryOptions {
    pub title: Option<String>,
    pub initial_directory: Option<PathBuf>,
}

trait PlatformBackend {
    async fn open_file_picker(opts: OpenFileOptions) -> Result<Option<Vec<PlatformFile>>, RlobKitError>;
    async fn open_directory_picker(opts: OpenDirectoryOptions) -> Result<Option<PlatformDirectory>, RlobKitError>;
    async fn open_file_saver(opts: SaveFileOptions) -> Result<Option<PlatformFile>, RlobKitError>;
    fn write_file_from_path(target: &PlatformFile, source: &Path) -> Result<(), RlobKitError>;
    fn read_file_to_path(source: &PlatformFile, dest: &Path) -> Result<(), RlobKitError>;
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
struct Backend;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl PlatformBackend for Backend {
    async fn open_file_picker(opts: OpenFileOptions) -> Result<Option<Vec<PlatformFile>>, RlobKitError> {
        crate::desktop::open_file_picker(opts).await
    }

    async fn open_directory_picker(opts: OpenDirectoryOptions) -> Result<Option<PlatformDirectory>, RlobKitError> {
        crate::desktop::open_directory_picker(opts).await
    }

    async fn open_file_saver(opts: SaveFileOptions) -> Result<Option<PlatformFile>, RlobKitError> {
        crate::desktop::open_file_saver(opts).await
    }

    fn write_file_from_path(target: &PlatformFile, source: &Path) -> Result<(), RlobKitError> {
        crate::desktop::write_file_from_path(target, source)
    }

    fn read_file_to_path(source: &PlatformFile, dest: &Path) -> Result<(), RlobKitError> {
        crate::desktop::read_file_to_path(source, dest)
    }
}

#[cfg(target_arch = "wasm32")]
struct Backend;

#[cfg(target_arch = "wasm32")]
impl PlatformBackend for Backend {
    async fn open_file_picker(opts: OpenFileOptions) -> Result<Option<Vec<PlatformFile>>, RlobKitError> {
        crate::wasm::open_file_picker(opts).await
    }

    async fn open_directory_picker(_opts: OpenDirectoryOptions) -> Result<Option<PlatformDirectory>, RlobKitError> {
        Err(RlobKitError::UnsupportedOperation("Directory picker not supported on WASM".into()))
    }

    async fn open_file_saver(opts: SaveFileOptions) -> Result<Option<PlatformFile>, RlobKitError> {
        crate::wasm::open_file_saver(opts).await
    }

    fn write_file_from_path(_target: &PlatformFile, _source: &Path) -> Result<(), RlobKitError> {
        Err(RlobKitError::UnsupportedOperation("Filesystem copy is not supported on WASM".into()))
    }

    fn read_file_to_path(_source: &PlatformFile, _dest: &Path) -> Result<(), RlobKitError> {
        Err(RlobKitError::UnsupportedOperation("Filesystem copy is not supported on WASM".into()))
    }
}

#[cfg(target_os = "android")]
struct Backend;

#[cfg(target_os = "android")]
impl PlatformBackend for Backend {
    async fn open_file_picker(opts: OpenFileOptions) -> Result<Option<Vec<PlatformFile>>, RlobKitError> {
        crate::android::open_file_picker(opts).await
    }

    async fn open_directory_picker(opts: OpenDirectoryOptions) -> Result<Option<PlatformDirectory>, RlobKitError> {
        crate::android::open_directory_picker(opts).await
    }

    async fn open_file_saver(opts: SaveFileOptions) -> Result<Option<PlatformFile>, RlobKitError> {
        crate::android::open_file_saver(opts).await
    }

    fn write_file_from_path(target: &PlatformFile, source: &Path) -> Result<(), RlobKitError> {
        crate::android::write_file_from_path(target, source)
    }

    fn read_file_to_path(source: &PlatformFile, dest: &Path) -> Result<(), RlobKitError> {
        crate::android::read_file_to_path(source, dest)
    }
}

pub struct RlobKit;

impl RlobKit {
    pub async fn open_file_picker(
        opts: OpenFileOptions,
    ) -> Result<Option<Vec<PlatformFile>>, RlobKitError> {
        Backend::open_file_picker(opts).await
    }

    pub async fn open_single_file(
        file_type: RlobKitType,
    ) -> Result<Option<PlatformFile>, RlobKitError> {
        let result = Self::open_file_picker(OpenFileOptions {
            file_type,
            mode: RlobKitMode::Single,
            ..Default::default()
        })
        .await?;
        Ok(result.and_then(|v| v.into_iter().next()))
    }

    pub async fn open_directory_picker(
        opts: OpenDirectoryOptions,
    ) -> Result<Option<PlatformDirectory>, RlobKitError> {
        Backend::open_directory_picker(opts).await
    }

    pub async fn open_file_saver(
        opts: SaveFileOptions,
    ) -> Result<Option<PlatformFile>, RlobKitError> {
        Backend::open_file_saver(opts).await
    }

    pub fn write_file_from_path(
        target: &PlatformFile,
        source_path: &Path,
    ) -> Result<(), RlobKitError> {
        Backend::write_file_from_path(target, source_path)
    }

    pub fn read_file_to_path(source: &PlatformFile, dest_path: &Path) -> Result<(), RlobKitError> {
        Backend::read_file_to_path(source, dest_path)
    }

    pub async fn save_bytes(
        opts: SaveFileOptions,
        data: &[u8],
    ) -> Result<Option<PlatformFile>, RlobKitError> {
        #[cfg(target_os = "android")]
        {
            let target = Self::open_file_saver(opts).await?;
            if let Some(file) = &target {
                let temp_name = if file.name().is_empty() {
                    "export.bin".to_string()
                } else {
                    file.name().to_string()
                };
                let temp = std::env::temp_dir().join(temp_name);
                std::fs::write(&temp, data)?;
                Self::write_file_from_path(file, &temp)?;
                let _ = std::fs::remove_file(&temp);
            }
            return Ok(target);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let mut wasm_opts = opts;
            wasm_opts.data = Some(data.to_vec());
            return Self::open_file_saver(wasm_opts).await;
        }

        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            let target = Self::open_file_saver(opts).await?;
            if let Some(file) = &target {
                file.write_bytes(data)?;
            }
            return Ok(target);
        }

        #[allow(unreachable_code)]
        Err(RlobKitError::UnsupportedOperation(
            "Unsupported platform".into(),
        ))
    }
}
