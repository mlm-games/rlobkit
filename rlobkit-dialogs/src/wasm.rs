use crate::picker::{OpenFileOptions, SaveFileOptions};
use crate::RlobKitMode;
use bytes::Bytes;
use rfd::AsyncFileDialog;
use rlobkit_core::{PlatformFile, RlobKitError};

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
                let name = handle.file_name();
                let data = Bytes::from(handle.read().await);
                Some(vec![PlatformFile::from_blob(name, data)])
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
                    let name = handle.file_name();
                    let data = Bytes::from(handle.read().await);
                    files.push(PlatformFile::from_blob(name, data));
                }
                if files.is_empty() {
                    None
                } else {
                    Some(files)
                }
            }
            None => None,
        },
    };

    Ok(files)
}

pub async fn open_file_saver(_opts: SaveFileOptions) -> Result<Option<PlatformFile>, RlobKitError> {
    Err(RlobKitError::UnsupportedOperation(
        "Save dialog on WASM requires web-sys download trigger".into(),
    ))
}
