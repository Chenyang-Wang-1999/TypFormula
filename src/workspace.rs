//! Project-scoped paths for the desktop backend.
//!
//! Every request that names a file resolves it here first: the window's document
//! directory is the compilation root, and nothing outside it can be addressed.
use std::path::{Component, Path, PathBuf};

pub fn resolve(root: &Path, name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(name);
    if name.is_empty() || relative.components().any(|c| !matches!(c, Component::Normal(_))) { return Err("需要项目内的相对路径".into()); }
    let root = root.canonicalize().map_err(|e|e.to_string())?;
    let candidate = root.join(relative);
    let existing = if candidate.exists() { candidate.clone() } else { candidate.parent().ok_or("缺少目录")?.to_path_buf() };
    if !existing.canonicalize().map_err(|e|e.to_string())?.starts_with(&root) { return Err("文件超出项目目录".into()); }
    Ok(candidate)
}
