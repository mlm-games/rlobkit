use crate::picker::{OpenDirectoryOptions, OpenFileOptions, SaveFileOptions};
use crate::{RlobKitMode, types::RlobKitType};
use rfd::AsyncFileDialog;
use rlobkit_core::{PlatformDirectory, PlatformFile, RlobKitError};
use std::io;
use std::path::Path;

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
    // Pass suggested name as-is - let file picker handle extensions
    if let Some(name) = &opts.suggested_name {
        dialog = dialog.set_file_name(name);
    }
    // Add filters - rfd handles showing them appropriately per platform
    if let Some(ft) = &opts.file_type {
        let exts = ft.extensions();
        for ext in &exts {
            let name = ext.to_uppercase();
            dialog = dialog.add_filter(&name, &[ext]);
        }
    }

    Ok(dialog
        .save_file()
        .await
        .map(|f| PlatformFile::from_path(f.path().to_path_buf())))
}

pub fn write_file_from_path(target: &PlatformFile, source_path: &Path) -> Result<(), RlobKitError> {
    let dest_path = target.path().ok_or_else(|| {
        RlobKitError::UnsupportedOperation("Desktop target is not a filesystem path".into())
    })?;

    let source = std::fs::canonicalize(source_path).unwrap_or_else(|_| source_path.to_path_buf());
    let destination = dest_path.to_path_buf();

    if source == destination {
        return Ok(());
    }

    std::fs::copy(&source, &destination)
        .map(|_| ())
        .map_err(|e| {
            RlobKitError::Io(io::Error::new(
                e.kind(),
                format!(
                    "Failed to copy '{}' to '{}': {e}",
                    source.display(),
                    destination.display()
                ),
            ))
        })
}

pub fn read_file_to_path(source: &PlatformFile, dest_path: &Path) -> Result<(), RlobKitError> {
    let src_path = source.path().ok_or_else(|| {
        RlobKitError::UnsupportedOperation("Desktop source is not a filesystem path".into())
    })?;

    let source = std::fs::canonicalize(src_path).unwrap_or_else(|_| src_path.to_path_buf());
    std::fs::copy(&source, dest_path).map(|_| ()).map_err(|e| {
        RlobKitError::Io(io::Error::new(
            e.kind(),
            format!(
                "Failed to copy '{}' to '{}': {e}",
                source.display(),
                dest_path.display()
            ),
        ))
    })
}
