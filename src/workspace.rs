//! Project-scoped files and optimistic saves. Never overwrite an externally changed file.
use std::{fs, path::{Component, Path, PathBuf}};
use serde_json::{Value, json};
static SAVES: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn resolve(root: &Path, name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(name);
    if name.is_empty() || relative.components().any(|c| !matches!(c, Component::Normal(_))) { return Err("需要项目内的相对路径".into()); }
    let root = root.canonicalize().map_err(|e|e.to_string())?;
    let candidate = root.join(relative);
    let existing = if candidate.exists() { candidate.clone() } else { candidate.parent().ok_or("缺少目录")?.to_path_buf() };
    if !existing.canonicalize().map_err(|e|e.to_string())?.starts_with(&root) { return Err("文件超出项目目录".into()); }
    Ok(candidate)
}
pub fn files(root: &Path) -> Result<Value,String> {
    fn walk(root:&Path, dir:&Path, out:&mut Vec<String>, depth:usize) -> Result<(),String> {
        if depth>12 || out.len()>4000 { return Ok(()); }
        for entry in fs::read_dir(dir).map_err(|e|e.to_string())? {
            let entry=entry.map_err(|e|e.to_string())?; let kind=entry.file_type().map_err(|e|e.to_string())?;
            if kind.is_symlink() || entry.file_name().to_string_lossy().starts_with('.') { continue; }
            if kind.is_dir() { if !["target","node_modules","vendor"].contains(&entry.file_name().to_string_lossy().as_ref()) { walk(root,&entry.path(),out,depth+1)?; } }
            else if entry.path().extension().is_some_and(|e|["typ","bib","json","yaml","yml","csv","toml","txt"].iter().any(|x|e==*x)) { out.push(entry.path().strip_prefix(root).unwrap().to_string_lossy().replace('\\',"/")); }
        } Ok(())
    }
    let mut result=vec![]; walk(root,root,&mut result,0)?; result.sort(); Ok(json!({"root":root,"files":result}))
}
pub fn file(root:&Path, req:Value) -> Result<Value,String> {
    let path=resolve(root,req["path"].as_str().ok_or("缺少路径")?)?;
    if let Some(source)=req["source"].as_str() {
        let _guard=SAVES.lock().map_err(|e|e.to_string())?;
        let current=fs::read_to_string(&path).ok();
        if current.as_deref()!=req["base"].as_str() { return Err("磁盘文件已改变，请重新打开或另存为以保留两份修改".into()); }
        if path.exists() && current.is_none() { return Err("无法读取已有文件".into()); }
        fs::write(path,source).map_err(|e|e.to_string())?; Ok(json!({"saved":true}))
    } else { Ok(json!({"source":fs::read_to_string(path).map_err(|e|e.to_string())?})) }
}
