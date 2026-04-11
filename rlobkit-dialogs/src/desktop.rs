use crate::picker::{OpenDirectoryOptions, OpenFileOptions, SaveFileOptions};
use crate::{types::RlobKitType, RlobKitMode};
use rfd::AsyncFileDialog;
use rlobkit_core::{PlatformDirectory, PlatformFile, RlobKitError};

pub async fn open_file_picker(
    opts: OpenFileOptions,
) -> Result<Option<Vec<PlatformFile>>, RlobKitError> {
    let mut dialog = AsyncFileDialog::new();

    if let Some(title) = &opts.title {
        dialog = dialog.set_title(title);
    }
    if let Some(dir) = &opts.initial_directory {
        dialog = dialog.set_directory(dir);
    }

    let exts = opts.file_type.extensions();
    if !exts.is_empty() {
        let filter_name = match opts.file_type {
            RlobKitType::Image => "Image",
            RlobKitType::Video => "Video",
            RlobKitType::ImageAndVideo => "Media",
            _ => "Files",
        };
        dialog = dialog.add_filter(filter_name, &exts);
    }

    let files = match opts.mode {
        RlobKitMode::Single => dialog
            .pick_file()
            .await
            .map(|f| vec![PlatformFile::from_path(f.path().to_path_buf())]),
        RlobKitMode::Multiple { limit } => {
            let files = dialog.pick_files().await;
            if let Some(files) = files {
                let mut result = Vec::new();
                for f in files {
                    if let Some(l) = limit {
                        if result.len() >= l {
                            break;
                        }
                    }
                    result.push(PlatformFile::from_path(f.path().to_path_buf()));
                }
                if result.is_empty() {
                    None
                } else {
                    Some(result)
                }
            } else {
                None
            }
        }
    };

    Ok(files)
}

pub async fn open_directory_picker(
    opts: OpenDirectoryOptions,
) -> Result<Option<PlatformDirectory>, RlobKitError> {
    let mut dialog = AsyncFileDialog::new();

    if let Some(title) = &opts.title {
        dialog = dialog.set_title(title);
    }
    if let Some(dir) = &opts.initial_directory {
        dialog = dialog.set_directory(dir);
    }

    Ok(dialog
        .pick_folder()
        .await
        .map(|f| PlatformDirectory::new(f.path().to_path_buf())))
}

pub async fn open_file_saver(opts: SaveFileOptions) -> Result<Option<PlatformFile>, RlobKitError> {
    let mut dialog = AsyncFileDialog::new();

    if let Some(title) = &opts.title {
        dialog = dialog.set_title(title);
    }
    if let Some(dir) = &opts.initial_directory {
        dialog = dialog.set_directory(dir);
    }
    if let Some(name) = &opts.suggested_name {
        dialog = dialog.set_file_name(name);
    }
    if let Some(ext) = &opts.extension {
        dialog = dialog.add_filter("File", &[ext.as_str()]);
    }

    Ok(dialog
        .save_file()
        .await
        .map(|f| PlatformFile::from_path(f.path().to_path_buf())))
}
