use std::{collections::BTreeMap, env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=config/symbols.json");
    let input = fs::read_to_string("config/symbols.json").expect("无法读取 config/symbols.json");
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
