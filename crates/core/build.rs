use std::{collections::BTreeMap, env, fs, path::PathBuf};

/// `config/symbols.json` belongs to the repository, not to this crate, so it is
/// addressed from the manifest directory. The build script runs with the crate
/// root as its working directory, which is one level deeper than it used to be.
fn symbols_json() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config/symbols.json")
}

fn main() {
    let input_path = symbols_json();
    println!("cargo:rerun-if-changed={}", input_path.display());
    let input = fs::read_to_string(&input_path)
        .unwrap_or_else(|error| panic!("无法读取 {}：{error}", input_path.display()));
    let entries: BTreeMap<String, String> = serde_json::from_str(&input)
        .expect("config/symbols.json 必须是源码片段到显示字符串的 JSON 字典");
    let mut generated = String::from("pub const SYMBOLS: &[(&str, &str)] = &[\n");
    for (source, glyph) in entries {
        assert!(!source.is_empty() && !glyph.is_empty(), "符号映射的源码和显示字符串不能为空");
        generated.push_str(&format!("    ({source:?}, {glyph:?}),\n"));
    }
    generated.push_str("];\n");
    fs::write(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("symbols.rs"), generated)
        .expect("无法生成符号映射");
}
