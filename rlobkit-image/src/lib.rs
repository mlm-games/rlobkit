//! rlobkit-image: Image compression and conversion utilities.

use bytes::Bytes;
use image::{ImageFormat, imageops::FilterType};
use rlobkit_core::RlobKitError;
use std::io::Cursor;

#[derive(Debug, Clone)]
pub struct CompressOptions {
    pub quality: u8,
    pub max_width: u32,
    pub max_height: u32,
    pub format: CompressFormat,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            quality: 80,
            max_width: 1920,
            max_height: 1080,
            format: CompressFormat::Jpeg,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub enum CompressFormat {
    #[default]
    Jpeg,
    Png,
    WebP,
}

pub fn compress_image(input: &[u8], opts: CompressOptions) -> Result<Bytes, RlobKitError> {
    let img = image::load_from_memory(input).map_err(|e| RlobKitError::Image(e.to_string()))?;

    let img = if img.width() > opts.max_width || img.height() > opts.max_height {
        img.resize(opts.max_width, opts.max_height, FilterType::Lanczos3)
    } else {
        img
    };

    let mut buf = Cursor::new(Vec::new());
    match opts.format {
        CompressFormat::Jpeg => {
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, opts.quality);
            encoder
                .encode_image(&img)
                .map_err(|e| RlobKitError::Image(e.to_string()))?;
        }
        CompressFormat::Png => {
            img.write_to(&mut buf, ImageFormat::Png)
                .map_err(|e| RlobKitError::Image(e.to_string()))?;
        }
        CompressFormat::WebP => {
            img.write_to(&mut buf, ImageFormat::WebP)
                .map_err(|e| RlobKitError::Image(e.to_string()))?;
        }
    }

    Ok(Bytes::from(buf.into_inner()))
}
