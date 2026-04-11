use crate::picker::{OpenDirectoryOptions, OpenFileOptions, SaveFileOptions};
use rlobkit_core::{PlatformDirectory, PlatformFile, RlobKitError};

pub async fn open_file_picker(
    _opts: OpenFileOptions,
) -> Result<Option<Vec<PlatformFile>>, RlobKitError> {
    Err(RlobKitError::UnsupportedOperation(
        "Android file picker requires JNI implementation".into(),
    ))
}

pub async fn open_directory_picker(
    _opts: OpenDirectoryOptions,
) -> Result<Option<PlatformDirectory>, RlobKitError> {
    Err(RlobKitError::UnsupportedOperation(
        "Android directory picker requires JNI implementation".into(),
    ))
}

pub async fn open_file_saver(_opts: SaveFileOptions) -> Result<Option<PlatformFile>, RlobKitError> {
    Err(RlobKitError::UnsupportedOperation(
        "Android file saver requires JNI implementation".into(),
    ))
}
