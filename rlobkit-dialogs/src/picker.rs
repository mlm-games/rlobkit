use crate::{RlobKitMode, RlobKitType};
use rlobkit_core::{PlatformDirectory, PlatformFile, RlobKitError};
use std::path::PathBuf;

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
}
