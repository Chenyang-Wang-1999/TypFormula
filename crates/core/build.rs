use std::{collections::BTreeMap, env, fs, path::PathBuf};

/// `config/` belongs to the repository, not to this crate, so it is addressed from
/// the manifest directory. The build script runs with the crate root as its working
/// directory, which is one level deeper than it used to be.
fn config(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config").join(name)
}

fn read(name: &str) -> String {
    let path = config(name);
    println!("cargo:rerun-if-changed={}", path.display());
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("无法读取 {}：{error}", path.display()))
}

/// `config/symbols.json`: a source fragment to the glyph it is drawn as.
fn symbols() -> String {
    let entries: BTreeMap<String, String> = serde_json::from_str(&read("symbols.json"))
        .expect("config/symbols.json 必须是源码片段到显示字符串的 JSON 字典");
    let mut out = String::from("pub const SYMBOLS: &[(&str, &str)] = &[\n");
    for (source, glyph) in entries {
        assert!(!source.is_empty() && !glyph.is_empty(), "符号映射的源码和显示字符串不能为空");
        out.push_str(&format!("    ({source:?}, {glyph:?}),\n"));
    }
    out.push_str("];\n");
    out
}

/// `config/commands.json`: a command name to the `Kind` it declares.
///
/// The viewer names are the same strings the wire uses (`slots::Decl::view`), so the
/// file can be read against the tables in `docs/kind-inventory.md` without a lookup.
fn commands() -> String {
    let entries: BTreeMap<String, String> = serde_json::from_str(&read("commands.json"))
        .expect("config/commands.json 必须是命令名到 Kind 视图名的 JSON 字典");
    let mut out = String::from("pub const COMMANDS: &[(&str, &str)] = &[\n");
    for (name, view) in entries {
        assert!(!name.is_empty() && !view.is_empty(), "命令名和它的视图名不能为空");
        assert!(view.chars().all(|c| c.is_ascii_lowercase() || c == '-'), "{view} 不是视图名");
        out.push_str(&format!("    ({name:?}, {view:?}),\n"));
    }
    out.push_str("];\n");
    out
}

fn main() {
    let generated = format!("{}\n{}", symbols(), commands());
    fs::write(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("config.rs"), generated)
        .expect("无法生成配置表");
}
