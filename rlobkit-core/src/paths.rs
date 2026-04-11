use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
pub fn files_dir(app_name: &str) -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join(app_name))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn cache_dir(app_name: &str) -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(app_name))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn downloads_dir() -> Option<PathBuf> {
    dirs::download_dir()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn pictures_dir() -> Option<PathBuf> {
    dirs::picture_dir()
}
