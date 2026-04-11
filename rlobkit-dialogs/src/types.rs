#[derive(Debug, Clone, Default)]
pub enum RlobKitType {
    #[default]
    Any,
    Image,
    Video,
    ImageAndVideo,
    Custom {
        extensions: Vec<String>,
        mime_types: Vec<String>,
    },
}

impl RlobKitType {
    pub fn extensions(&self) -> Vec<&str> {
        match self {
            Self::Any => vec![],
            Self::Image => vec!["png", "jpg", "jpeg", "gif", "webp", "bmp", "heic"],
            Self::Video => vec!["mp4", "mov", "avi", "mkv", "webm"],
            Self::ImageAndVideo => vec![
                "png", "jpg", "jpeg", "gif", "webp", "bmp", "heic", "mp4", "mov", "avi", "mkv",
                "webm",
            ],
            Self::Custom { extensions, .. } => extensions.iter().map(String::as_str).collect(),
        }
    }

    pub fn mime_types(&self) -> Vec<&str> {
        match self {
            Self::Any => vec!["*/*"],
            Self::Image => vec!["image/*"],
            Self::Video => vec!["video/*"],
            Self::ImageAndVideo => vec!["image/*", "video/*"],
            Self::Custom { mime_types, .. } => mime_types.iter().map(String::as_str).collect(),
        }
    }
}
