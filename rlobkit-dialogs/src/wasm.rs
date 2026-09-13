use crate::RlobKitMode;
use crate::picker::{OpenFileOptions, SaveFileOptions};
use bytes::Bytes;
use rfd::AsyncFileDialog;
use rlobkit_core::{PlatformFile, RlobKitError};
use std::num::NonZeroUsize;
use std::path::Path;
use wasm_bindgen::JsCast;

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
                let limit = limit.map(NonZeroUsize::get);
                for handle in handles {
                    if let Some(l) = limit
                        && files.len() >= l
                    {
                        break;
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
    if let Some(data) = opts.data {
        let name = opts
            .suggested_name
            .clone()
            .unwrap_or_else(|| "file.bin".to_string());

        let window = web_sys::window().ok_or_else(|| {
            RlobKitError::Io(std::io::Error::new(std::io::ErrorKind::Other, "no window"))
        })?;
        let document = window.document().ok_or_else(|| {
            RlobKitError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "no document",
            ))
        })?;
        let array = js_sys::Array::new();

        let uint8arr = js_sys::Uint8Array::new_with_length(data.len() as u32);
        uint8arr.copy_from(&data);
        array.push(&uint8arr);
        let blob = web_sys::Blob::new_with_u8_array_sequence(&array).map_err(|e| {
            RlobKitError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("blob creation failed: {e:?}"),
            ))
        })?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| {
            RlobKitError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("create_object_url failed: {e:?}"),
            ))
        })?;
        let body = document.body().ok_or_else(|| {
            RlobKitError::Io(std::io::Error::new(std::io::ErrorKind::Other, "no body"))
        })?;
        let anchor: web_sys::HtmlAnchorElement = document
            .create_element("a")
            .map_err(|_| {
                RlobKitError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "createElement failed",
                ))
            })?
            .dyn_into()
            .map_err(|_| {
                RlobKitError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "cast to anchor failed",
                ))
            })?;
        anchor.set_href(&url);
        anchor.set_download(&name);
        anchor.style().set_property("display", "none").ok();
        body.append_child(&anchor).map_err(|_| {
            RlobKitError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "appendChild failed",
            ))
        })?;
        anchor.click();

        let url_clone = url.clone();
        let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
            web_sys::Url::revoke_object_url(&url_clone).ok();
        });
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                revoke.as_ref().unchecked_ref(),
                60_000,
            );
        } else {
            web_sys::Url::revoke_object_url(&url).ok();
        }

        anchor.remove();
        return Ok(Some(PlatformFile::from_blob(name, Bytes::new(), None)));
    }

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
