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
    #[cfg(target_arch = "wasm32")]
    pub data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct OpenDirectoryOptions {
    pub title: Option<String>,
    pub initial_directory: Option<PathBuf>,
}

pub struct RlobKit;

impl RlobKit {
    pub async fn open_file_picker(
        opts: OpenFileOptions,
    ) -> Result<Option<Vec<PlatformFile>>, RlobKitError> {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            return crate::desktop::open_file_picker(opts).await;
        }

        #[cfg(target_arch = "wasm32")]
        {
            return crate::wasm::open_file_picker(opts).await;
        }

        #[cfg(target_os = "android")]
        {
            return crate::android::open_file_picker(opts).await;
        }

        #[allow(unreachable_code)]
        Err(RlobKitError::UnsupportedOperation(
            "Unsupported platform".into(),
        ))
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
        Ok(result.and_then(|mut v| {
            if v.is_empty() {
                None
            } else {
                Some(v.remove(0))
            }
        }))
    }

    pub async fn open_directory_picker(
        opts: OpenDirectoryOptions,
    ) -> Result<Option<PlatformDirectory>, RlobKitError> {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            return crate::desktop::open_directory_picker(opts).await;
        }

        #[cfg(target_arch = "wasm32")]
        {
            return Err(RlobKitError::UnsupportedOperation(
                "Directory picker not supported on WASM".into(),
            ));
        }

        #[cfg(target_os = "android")]
        {
            return crate::android::open_directory_picker(opts).await;
        }

        #[allow(unreachable_code)]
        Err(RlobKitError::UnsupportedOperation(
            "Unsupported platform".into(),
        ))
    }

    pub async fn open_file_saver(
        opts: SaveFileOptions,
    ) -> Result<Option<PlatformFile>, RlobKitError> {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            return crate::desktop::open_file_saver(opts).await;
        }

        #[cfg(target_arch = "wasm32")]
        {
            return crate::wasm::open_file_saver(opts).await;
        }

        #[cfg(target_os = "android")]
        {
            return crate::android::open_file_saver(opts).await;
        }

        #[allow(unreachable_code)]
        Err(RlobKitError::UnsupportedOperation(
            "Unsupported platform".into(),
        ))
    }

    pub fn write_file_from_path(
        target: &PlatformFile,
        source_path: &Path,
    ) -> Result<(), RlobKitError> {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            return crate::desktop::write_file_from_path(target, source_path);
        }

        #[cfg(target_arch = "wasm32")]
        {
            return crate::wasm::write_file_from_path(target, source_path);
        }

        #[cfg(target_os = "android")]
        {
            return crate::android::write_file_from_path(target, source_path);
        }

        #[allow(unreachable_code)]
        Err(RlobKitError::UnsupportedOperation(
            "Unsupported platform".into(),
        ))
    }

    pub fn read_file_to_path(
        source: &PlatformFile,
        dest_path: &Path,
    ) -> Result<(), RlobKitError> {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            return crate::desktop::read_file_to_path(source, dest_path);
        }

        #[cfg(target_arch = "wasm32")]
        {
            return crate::wasm::read_file_to_path(source, dest_path);
        }

        #[cfg(target_os = "android")]
        {
            return crate::android::read_file_to_path(source, dest_path);
        }

        #[allow(unreachable_code)]
        Err(RlobKitError::UnsupportedOperation(
            "Unsupported platform".into(),
        ))
    }
}
