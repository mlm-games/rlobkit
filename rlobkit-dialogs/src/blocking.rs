#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
use crate::RlobKit;
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
use crate::RlobKitType;
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
use crate::mode::RlobKitMode;
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
use crate::picker::{OpenDirectoryOptions, OpenFileOptions, SaveFileOptions};
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
use std::path::PathBuf;

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
fn block_on_runtime<T>(future: impl std::future::Future<Output = T>) -> T {
    futures_lite::future::block_on(future)
}

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
pub fn blocking_open_file(title: &str, extensions: &[&str]) -> Option<PathBuf> {
    let exts: Vec<String> = extensions.iter().map(|s| s.to_string()).collect();
    block_on_runtime(async {
        let result = RlobKit::open_file_picker(OpenFileOptions {
            file_type: RlobKitType::Custom {
                extensions: exts,
                mime_types: vec!["*/*".to_string()],
            },
            mode: RlobKitMode::Single,
            title: Some(title.to_string()),
            initial_directory: None,
        })
        .await;
        match result {
            Ok(Some(mut files)) if !files.is_empty() => {
                files.pop().and_then(|f| f.path().map(|p| p.to_path_buf()))
            }
            _ => None,
        }
    })
}

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
pub fn blocking_save_file(title: &str, suggested_name: &str, extension: &str) -> Option<PathBuf> {
    block_on_runtime(async {
        let exts: Vec<String> = extension
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let result = RlobKit::open_file_saver(SaveFileOptions {
            suggested_name: Some(suggested_name.to_string()),
            extension: None,
            file_type: Some(RlobKitType::Custom {
                extensions: exts,
                mime_types: vec![],
            }),
            title: Some(title.to_string()),
            initial_directory: None,
        })
        .await;
        match result {
            Ok(Some(f)) => f.path().map(|p| p.to_path_buf()),
            _ => None,
        }
    })
}

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
pub fn blocking_pick_files(title: &str, extensions: &[&str]) -> Vec<PathBuf> {
    let exts: Vec<String> = extensions.iter().map(|s| s.to_string()).collect();
    block_on_runtime(async {
        let result = RlobKit::open_file_picker(OpenFileOptions {
            file_type: RlobKitType::Custom {
                extensions: exts,
                mime_types: vec!["*/*".to_string()],
            },
            mode: RlobKitMode::Multiple { limit: None },
            title: Some(title.to_string()),
            initial_directory: None,
        })
        .await;
        match result {
            Ok(Some(files)) => files
                .into_iter()
                .filter_map(|f| f.path().map(|p| p.to_path_buf()))
                .collect(),
            _ => Vec::new(),
        }
    })
}

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
pub fn blocking_pick_directory(title: &str) -> Option<PathBuf> {
    block_on_runtime(async {
        let result = RlobKit::open_directory_picker(OpenDirectoryOptions {
            title: Some(title.to_string()),
            initial_directory: None,
        })
        .await;
        match result {
            Ok(Some(dir)) => dir.path().map(|p| p.to_path_buf()),
            _ => None,
        }
    })
}
