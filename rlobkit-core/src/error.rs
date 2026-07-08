use thiserror::Error;

#[derive(Debug, Error)]
pub enum RlobKitError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Picker cancelled by user")]
    Cancelled,

    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("Invalid URI: {0}")]
    InvalidUri(String),

    #[error("Image error: {0}")]
    Image(String),

    #[error("Android JNI error: {0}")]
    AndroidJni(String),
}
