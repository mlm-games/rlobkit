use crate::RlobKitMode;
use crate::picker::{OpenFileOptions, SaveFileOptions};
use bytes::Bytes;
use rfd::AsyncFileDialog;
use rlobkit_core::{PlatformFile, RlobKitError};
use std::path::Path;

pub async fn open_file_picker(
    opts: OpenFileOptions,
) -> Result<Option<Vec<PlatformFile>>, RlobKitError> {
    let mut dialog = AsyncFileDialog::new();

    let exts = opts.file_type.extensions();
    if !exts.is_empty() {
        dialog = dialog.add_filter("files", &exts);
    }
    if let Some(title) = &opts.title {
        dialog = dialog.set_title(title);
    }

    let files = match opts.mode {
        RlobKitMode::Single => match dialog.pick_file().await {
            Some(handle) => {
                let name = handle.file_name().to_string();
                let data = Bytes::from(handle.read().await);
                Some(vec![PlatformFile::from_blob(name, data, None)])
            }
            None => None,
        },
        RlobKitMode::Multiple { limit } => match dialog.pick_files().await {
            Some(handles) => {
                let mut files = Vec::new();
                for handle in handles {
                    if let Some(l) = limit {
                        if files.len() >= l {
                            break;
                        }
                    }
                    let name = handle.file_name().to_string();
                    let data = Bytes::from(handle.read().await);
                    files.push(PlatformFile::from_blob(name, data, None));
                }
                if files.is_empty() { None } else { Some(files) }
            }
            None => None,
        },
    };

    Ok(files)
}

pub async fn open_file_saver(opts: SaveFileOptions) -> Result<Option<PlatformFile>, RlobKitError> {
    let mut dialog = AsyncFileDialog::new();

    if let Some(title) = &opts.title {
        dialog = dialog.set_title(title);
    }
    if let Some(name) = &opts.suggested_name {
        dialog = dialog.set_file_name(name);
    }

    let file = match dialog.save_file().await {
        Some(f) => f,
        None => return Ok(None),
    };

    if let Some(data) = opts.data {
        let _ = file.write(&data).await;
    }

    let name = file.file_name().to_string();
    Ok(Some(PlatformFile::from_blob(name, Bytes::new(), None)))
}

pub fn write_file_from_path(
    _target: &PlatformFile,
    _source_path: &Path,
) -> Result<(), RlobKitError> {
    Err(RlobKitError::UnsupportedOperation(
        "Filesystem copy is not supported on WASM".into(),
    ))
}

pub fn read_file_to_path(_source: &PlatformFile, _dest_path: &Path) -> Result<(), RlobKitError> {
    Err(RlobKitError::UnsupportedOperation(
        "Filesystem copy is not supported on WASM".into(),
    ))
}