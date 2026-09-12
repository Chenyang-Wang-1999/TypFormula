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

/// One entry of `config/commands.json`.
///
/// A name may be written as a bare shape name when that says everything, or as an
/// object when the drawing needs parameters. The object form exists because some
/// shapes cannot be described by a name alone:
///
/// * `vec` and `cases` are tables all right, but a table whose *arguments* are its
///   rows and whose delimiters are not the matrix default — neither fact is visible
///   in the string "grid";
/// * `abs` and `norm` share the `delim` shape and differ only in which pair they are
///   drawn with, so the pair has to be written down;
/// * `overline` and `underline` share the `line` shape and differ only in the side
///   the rule goes, and no node stores that side.
///
/// A field left out takes its default, which is how a name needing no parameters
/// stays a bare string.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Spec {
    /// A shape that needs no parameters.
    Plain(String),
    /// A shape plus the parameters its drawing needs.
    Detailed {
        shape: String,
        /// How the argument list becomes rows. Absent for every shape but a table.
        #[serde(default)]
        rows: Option<String>,
        /// The delimiters the frontend draws around a table, left then right
        /// (`"()"`), or a single character for a one-sided pair (`"{ "`).
        #[serde(default)]
        border: Option<String>,
        /// The delimiters a `delim`-shaped call is drawn with: left, newline, right
        /// (`"|\n|"`), the same spelling the frontend reads out of the view's `text`.
        #[serde(default)]
        text: Option<String>,
        /// Which side a `line`-shaped call draws its rule on.
        #[serde(default)]
        above: Option<bool>,
    },
}

impl Spec {
    fn shape(&self) -> &str {
        match self {
            Spec::Plain(shape) => shape,
            Spec::Detailed { shape, .. } => shape,
        }
    }
    fn rows(&self) -> Option<&str> {
        match self {
            Spec::Plain(_) => None,
            Spec::Detailed { rows, .. } => rows.as_deref(),
        }
    }
    fn border(&self) -> Option<&str> {
        match self {
            Spec::Plain(_) => None,
            Spec::Detailed { border, .. } => border.as_deref(),
        }
    }
    fn text(&self) -> Option<&str> {
        match self {
            Spec::Plain(_) => None,
            Spec::Detailed { text, .. } => text.as_deref(),
        }
    }
    fn above(&self) -> Option<bool> {
        match self {
            Spec::Plain(_) => None,
            Spec::Detailed { above, .. } => *above,
        }
    }
}

/// `config/commands.json`: a command name to the shape it declares, plus whatever
/// its drawing needs that the name alone does not say.
///
/// The shape names are the same strings `slots::Shape::view` uses, so the file can be
/// read against the tables in `docs/kind-inventory.md` without a lookup.
fn commands() -> String {
    let entries: BTreeMap<String, Spec> = serde_json::from_str(&read("commands.json"))
        .expect("config/commands.json 必须是命令名到形状名（或 {shape, …} 对象）的 JSON 字典");
    let mut out = String::from(
        "pub struct Command { pub name: &'static str, pub shape: &'static str, \
         pub rows: Option<&'static str>, pub border: Option<&'static str>, \
         pub text: Option<&'static str>, pub above: Option<bool> }\n\
         pub const COMMANDS: &[Command] = &[\n",
    );
    for (name, spec) in entries {
        let shape = spec.shape();
        assert!(!name.is_empty() && !shape.is_empty(), "命令名和它的形状名不能为空");
        assert!(shape.chars().all(|c| c.is_ascii_lowercase() || c == '-'), "{shape} 不是形状名");
        if let Some(rows) = spec.rows() {
            assert!(matches!(rows, "mat" | "each"), "{rows} 不是切行方式（mat / each）");
            assert_eq!(shape, "grid", "只有表格形状才谈得上怎么切行，{name} 却是 {shape}");
        }
        if let Some(border) = spec.border() {
            assert!(!border.is_empty() && border.chars().count() <= 2,
                    "{border:?} 不是定界符对（左+右，或单边一个）");
            assert_eq!(shape, "grid", "只有表格形状才谈得上定界符，{name} 却是 {shape}");
        }
        if let Some(text) = spec.text() {
            let lines: Vec<&str> = text.split('\n').collect();
            assert_eq!(lines.len(), 2, "{text:?} 不是定界符对（左，换行，右）");
            assert!(lines.iter().all(|line| !line.is_empty()), "{text:?} 有一边是空的");
            assert_eq!(shape, "delim", "只有 delim 形状才谈得上定界符文本，{name} 却是 {shape}");
        }
        if spec.above().is_some() {
            assert_eq!(shape, "line", "只有 line 形状才谈得上横线在哪一侧，{name} 却是 {shape}");
        }
        out.push_str(&format!(
            "    Command {{ name: {name:?}, shape: {shape:?}, rows: {:?}, border: {:?}, text: {:?}, above: {:?} }},\n",
            spec.rows(),
            spec.border(),
            spec.text(),
            spec.above()
        ));
    }
    out.push_str("];\n");
    out
}

fn main() {
    let generated = format!("{}\n{}", symbols(), commands());
    fs::write(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("config.rs"), generated)
        .expect("无法生成配置表");
}
