//! Official registry, explicit version installation, and the standard Typst cache.
use std::{fs, io::{Read, Cursor}, path::PathBuf, sync::Mutex};
use serde_json::{Value,json};
static INSTALL: Mutex<()> = Mutex::new(());
static INDEX: Mutex<Option<Value>> = Mutex::new(None);
pub fn cache() -> PathBuf {
    if let Some(p)=std::env::var_os("TYPST_PACKAGE_CACHE_PATH") { return p.into(); }
    if cfg!(windows) { PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("typst/packages") }
    else if cfg!(target_os="macos") { PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Library/Caches/typst/packages") }
    else { std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(||PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")).join("typst/packages") }
}
fn download(url:&str, limit:u64) -> Result<Vec<u8>,String> {
    let agent:ureq::Agent=ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(12))).build().into();
    let mut response=agent.get(url).call().map_err(|e|e.to_string())?;
    let mut bytes=vec![]; response.body_mut().as_reader().take(limit+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len() as u64>limit { return Err("包下载超过大小限制".into()); } Ok(bytes)
}
pub fn handle(req:Value) -> Result<Value,String> {
    if req["action"]=="local" {
        let mut roots=vec![];
        if let Some(path)=std::env::var_os("TYPST_PACKAGE_PATH") { roots.push(PathBuf::from(path)); }
        if let Some(path)=std::env::var_os("APPDATA") { roots.push(PathBuf::from(path).join("typst/packages")); }
        if let Some(home)=std::env::var_os("HOME") { let home=PathBuf::from(home);roots.extend([home.join(".local/share/typst/packages"),home.join("Library/Application Support/typst/packages")]); }
        let query=req["query"].as_str().unwrap_or("").to_lowercase();
        let mut items=vec![];let mut seen=std::collections::HashSet::new();
        for root in roots {
            let Ok(names)=fs::read_dir(root.join("local")) else {continue};
            for name in names.flatten() {
                let Ok(versions)=fs::read_dir(name.path()) else {continue};
                for version in versions.flatten() {
                    let name=name.file_name().to_string_lossy().into_owned();let version_name=version.file_name().to_string_lossy().into_owned();
                    let spec=format!("@local/{name}:{version_name}");
                    if !name.to_lowercase().contains(&query)||spec.parse::<typst_syntax::package::PackageSpec>().is_err()||!version.path().join("typst.toml").is_file()||!seen.insert(spec) {continue;}
                    items.push(json!({"namespace":"local","name":name,"version":version_name,"installed":true,"path":version.path()}));
                }
            }
        }
        items.sort_by_key(|v|format!("{}:{}",v["name"],v["version"]));
        return Ok(json!({"items":items}));
    }
    if req["action"]=="search" {
        let mut index=INDEX.lock().map_err(|e|e.to_string())?;
        if index.is_none() { *index=Some(serde_json::from_slice(&download("https://packages.typst.org/preview/index.json",16*1024*1024)?).map_err(|e|e.to_string())?); }
        let query=req["query"].as_str().unwrap_or("").to_lowercase();
        let items:Vec<_>=index.as_ref().unwrap().as_array().ok_or("包索引格式无效")?.iter().filter(|p|format!("{} {}",p["name"],p["description"]).to_lowercase().contains(&query)).rev().take(80).map(|p|{
            let mut item=p.clone(); item["installed"]=json!(cache().join("preview").join(p["name"].as_str().unwrap_or("")).join(p["version"].as_str().unwrap_or("")).join("typst.toml").is_file()); item
        }).collect();
        return Ok(json!({"items":items,"cache":cache()}));
    }
    if req["action"]!="install" { return Err("不支持的包操作".into()); }
    let spec:typst_syntax::package::PackageSpec=req["spec"].as_str().ok_or("缺少包版本")?.parse().map_err(|e|format!("{e}"))?;
    if spec.namespace!="preview" { return Err("安装仅支持官方 preview 包".into()); }
    let _guard=INSTALL.lock().map_err(|e|e.to_string())?;
    let base=cache().join("preview").join(spec.name.as_str()); let target=base.join(spec.version.to_string());
    if !target.join("typst.toml").is_file() {
        if target.exists() { return Err("缓存目录不完整，请先检查该目录".into()); }
        let bytes=download(&format!("https://packages.typst.org/preview/{}-{}.tar.gz",spec.name,spec.version),32*1024*1024)?;
        fs::create_dir_all(&base).map_err(|e|e.to_string())?;
        let staging=base.join(format!(".{}-{}",spec.version,std::process::id()));
        fs::create_dir(&staging).map_err(|e|e.to_string())?;
        let result=(|| {
            let mut archive=tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(bytes))); let mut size=0u64;
            for entry in archive.entries().map_err(|e|e.to_string())? {
                let mut entry=entry.map_err(|e|e.to_string())?; let kind=entry.header().entry_type();
                if !(kind.is_file()||kind.is_dir()) { return Err("包中不允许链接或特殊文件".to_string()); }
                size=size.saturating_add(entry.size()); if size>128*1024*1024 { return Err("解压后包过大".into()); }
                if !entry.unpack_in(&staging).map_err(|e|e.to_string())? { return Err("无效的包文件路径".into()); }
            }
            if !staging.join("typst.toml").is_file() { return Err("缺少 typst.toml".into()); }
            fs::rename(&staging,&target).map_err(|e|e.to_string())?; Ok(())
        })();
        if result.is_err() { let _=fs::remove_dir_all(&staging); } result?;
    }
    Ok(json!({"installed":true,"spec":spec.to_string(),"import":format!("#import \"{spec}\": *\n")}))
}
