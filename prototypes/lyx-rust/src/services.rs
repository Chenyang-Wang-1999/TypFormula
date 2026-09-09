// SPDX-License-Identifier: GPL-2.0-or-later
//! Native Tinymist bridge: isolated stdio LSP projections and SVG compilation.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, io::{BufRead, BufReader, Read, Write}, path::{Path, PathBuf}, process::{Child, ChildStdin, Command, Stdio}, sync::{Mutex, mpsc}, time::{Duration, Instant}};

fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
    command
}
pub fn find_tinymist() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("TINYMIST_BIN") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path).ok_or("TINYMIST_BIN 指向的程序不存在".into());
    }
    let exe = if cfg!(windows) { "tinymist.exe" } else { "tinymist" };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) { let path = dir.join(exe); if path.is_file() { return Ok(path); } }
    }
    if let Some(profile) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        for editor in [".vscode", ".vscode-insiders", ".cursor"] {
            let dir = PathBuf::from(&profile).join(editor).join("extensions");
            if let Ok(entries) = fs::read_dir(dir) {
                let mut found: Vec<_> = entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("myriad-dreamin.tinymist-")).map(|e| e.path().join("out").join(exe)).filter(|p| p.is_file()).collect();
                found.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
                if let Some(path) = found.pop() { return Ok(path); }
            }
        }
    }
    Err("未找到 Tinymist；请安装 VS Code Tinymist 扩展，或设置 TINYMIST_BIN".into())
}

struct Lsp { child: Child, input: ChildStdin, messages: mpsc::Receiver<Value>, next_id: u64, uri: String }
impl Drop for Lsp { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }
impl Lsp {
    fn start(bin: &Path, root: &Path) -> Result<Self, String> {
        let mut child = hidden(Command::new(bin).arg("lsp")).current_dir(root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, messages) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut length = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 { return; }
                    if line.trim().is_empty() { break; }
                    if let Some((key, value)) = line.split_once(':') { if key.eq_ignore_ascii_case("content-length") { length = value.trim().parse::<usize>().ok(); } }
                }
                let Some(n) = length.filter(|n| *n < 16 * 1024 * 1024) else { return; };
                let mut bytes = vec![0; n];
                if reader.read_exact(&mut bytes).is_err() { return; }
                if let Ok(value) = serde_json::from_slice(&bytes) { if tx.send(value).is_err() { return; } }
            }
        });
        let url = url::Url::from_directory_path(root).map_err(|_| "无效的项目路径")?;
        let uri = url.join("command-buffer.typ").map_err(|e| e.to_string())?.to_string();
        let mut lsp = Self { child, input, messages, next_id: 0, uri };
        lsp.request("initialize", json!({"processId":std::process::id(),"rootUri":url.as_str(),"capabilities":{"general":{"positionEncodings":["utf-16"]},"textDocument":{"completion":{"completionItem":{"snippetSupport":false}}}},"initializationOptions":{"exportPdf":"never","semanticTokens":"disable"}}))?;
        lsp.notify("initialized", json!({}))?;
        Ok(lsp)
    }
    fn write(&mut self, value: Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        write!(self.input, "Content-Length: {}\r\n\r\n", bytes.len()).and_then(|_| self.input.write_all(&bytes)).and_then(|_| self.input.flush()).map_err(|e| e.to_string())
    }
    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> { self.write(json!({"jsonrpc":"2.0","method":method,"params":params})) }
    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1; let id = self.next_id;
        self.write(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        let until = Instant::now() + Duration::from_secs(12);
        loop {
            let value = self.messages.recv_timeout(until.saturating_duration_since(Instant::now())).map_err(|_| "Tinymist 响应超时".to_string())?;
            if value.get("method").is_some() && value.get("id").is_some() {
                let result = if value["method"] == "workspace/configuration" { json!([]) } else { Value::Null };
                self.write(json!({"jsonrpc":"2.0","id":value["id"],"result":result}))?;
            } else if value["id"].as_u64() == Some(id) {
                if let Some(error) = value.get("error") { return Err(error.to_string()); }
                return Ok(value["result"].clone());
            }
        }
    }
}

#[derive(Deserialize)]
pub struct CompletionRequest { pub source: String, pub start: usize, pub end: usize, pub caret: usize }
#[derive(Serialize)]
pub struct CompletionReply { pub engine: &'static str, pub items: Vec<crate::cursor::CommandCompletion> }
#[derive(Deserialize)]
pub struct RenderRequest { pub expression: String, #[serde(default)] pub definitions: String, #[serde(default = "default_display")] pub display: bool }
fn default_display() -> bool { true }

#[derive(Deserialize, Serialize)]
pub struct AttachmentRequest { pub expression: String, #[serde(default)] pub definitions: String, pub display: bool }

pub struct Services { bin: Result<PathBuf, String>, root: PathBuf, completion_lock: Mutex<()> }
impl Services {
    pub fn new(root: PathBuf) -> Self { Self { bin: find_tinymist(), root, completion_lock: Mutex::new(()) } }
    fn adapter_bin(&self) -> PathBuf { self.root.join("target/adapter").join(if cfg!(debug_assertions) { "debug" } else { "release" }).join(if cfg!(windows) { "lyx-typst-layout-adapter.exe" } else { "lyx-typst-layout-adapter" }) }
    pub fn status(&self) -> Value { match &self.bin { Ok(path) => json!({"available":true,"attachments":self.adapter_bin().is_file(),"engine":"Tinymist LSP + Typst","path":path}), Err(error) => json!({"available":false,"attachments":self.adapter_bin().is_file(),"error":error}) } }
    pub fn attachments(&self, req: AttachmentRequest) -> Result<Value, String> {
        if !self.adapter_bin().is_file() { return Err("请运行 build-native.cmd 并重启服务，以启用 Typst limits/stretch 适配器".into()); }
        let body = serde_json::to_vec(&req).map_err(|e| e.to_string())?;
        let mut child = hidden(&mut Command::new(self.adapter_bin())).current_dir(&self.root)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
        let mut input = child.stdin.take().unwrap();
        // Put writes on a separate thread so even a stuck child cannot block
        // the request timeout. Dropping stdin tells the adapter the request ends.
        let writer = std::thread::spawn(move || input.write_all(&body));
        let stdout = child.stdout.take().unwrap(); let stderr = child.stderr.take().unwrap();
        let output = std::thread::spawn(move || { let mut b = vec![]; let _ = stdout.take(16*1024*1024).read_to_end(&mut b); b });
        let errors = std::thread::spawn(move || { let mut b = vec![]; let _ = stderr.take(1024*1024).read_to_end(&mut b); b });
        let until = Instant::now() + Duration::from_secs(12);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
                result => {
                    let _ = child.kill(); let _ = child.wait();
                    let _ = writer.join(); let _ = output.join(); let _ = errors.join();
                    return Err(match result { Err(e) => e.to_string(), _ => "Typst 附件布局超时；保留原编辑结构".into() });
                }
            }
        };
        let _ = writer.join();
        let bytes = output.join().map_err(|_| "无法读取附件布局")?;
        let error = errors.join().unwrap_or_default();
        if !status.success() { return Err(String::from_utf8_lossy(&error).into_owned()); }
        let result: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if let Some(error) = result["error"].as_str() { return Err(error.into()); }
        Ok(result)
    }
    pub fn complete(&self, mut req: CompletionRequest) -> Result<CompletionReply, String> {
        let bin = self.bin.as_ref().map_err(Clone::clone)?;
        if req.start > req.caret || req.caret > req.end || req.end > req.source.len() || ![req.start, req.caret, req.end].iter().all(|p| req.source.is_char_boundary(*p)) { return Err("补全位置无效".into()); }
        // Supply unfinished brackets only in the LSP projection. Draft bytes
        // and edit ranges are unchanged, and no brackets are committed to it.
        let suffix = closing_brackets(&req.source[req.start..req.end]);
        req.source.insert_str(req.end, &suffix);
        let _guard = self.completion_lock.lock().map_err(|e| e.to_string())?;
        // Tinymist 0.15.6 intermittently reuses a stale compiler world even
        // after switching/pinning in-memory documents. Isolate each projection
        // until a reusable-session implementation can guarantee synchronization.
        // Browser debouncing and its single in-flight request bound process use.
        let mut lsp = Lsp::start(bin, &self.root)?;
        lsp.notify("textDocument/didOpen", json!({"textDocument":{"uri":lsp.uri,"languageId":"typst","version":1,"text":req.source}}))?;
        lsp.request("workspace/executeCommand", json!({"command":"tinymist.pinMain","arguments":[url::Url::parse(&lsp.uri).unwrap().to_file_path().unwrap()]}))?;
        let mut attempt = 0;
        loop {
        let result = lsp.request("textDocument/completion", json!({"textDocument":{"uri":lsp.uri},"position":position(&req.source, req.caret),"context":{"triggerKind":1}}));
        let result = result?;
        let raw = result.as_array().or_else(|| result["items"].as_array());
        let mut items = Vec::new();
        if let Some(raw) = raw {
            for item in raw.iter().take(300) {
                let Some(label) = item["label"].as_str() else { continue; };
                if item.get("additionalTextEdits").and_then(Value::as_array).is_some_and(|a| !a.is_empty()) { continue; }
                let edit = &item["textEdit"];
                let range = edit.get("range").or_else(|| edit.get("replace"));
                let (start, end) = if let Some(range) = range {
                    let Some(start) = offset(&req.source, &range["start"]) else { continue; };
                    let Some(end) = offset(&req.source, &range["end"]) else { continue; }; (start, end)
                } else {
                    let prefix = &req.source[req.start..req.caret];
                    let start = prefix.char_indices().rev().find(|(_,c)| !c.is_alphanumeric() && *c != '_' && *c != '-').map_or(req.start, |(i,c)| req.start+i+c.len_utf8());
                    (start, req.caret)
                };
                if start < req.start || end > req.end || start > end { continue; }
                let text = edit["newText"].as_str().or_else(|| item["insertText"].as_str()).unwrap_or(label);
                // Tinymist labels even plain symbols as snippets. Convert simple
                // placeholders to text, keeping the first argument caret.
                let Some((text, cursor)) = (if item["insertTextFormat"].as_u64() == Some(2) { snippet_text(text) } else { Some((text.to_string(), text.len())) }) else { continue; };
                let mut replacement = req.source[req.start..req.end].to_string();
                replacement.replace_range(start-req.start..end-req.start, &text);
                items.push(crate::cursor::CommandCompletion { label: label.into(), replacement, caret: start-req.start+cursor });
            }
        }
        let prefix = req.source[req.start..req.caret].rsplit(|c:char| !c.is_alphanumeric() && c != '_' && c != '-').next().unwrap_or("");
        items.sort_by_key(|i| (!i.label.starts_with(prefix), i.label.clone()));
        items.dedup_by(|a,b| a.label == b.label);
        if items.is_empty() && attempt < 5 { attempt += 1; std::thread::sleep(Duration::from_millis(60)); continue; }
        return Ok(CompletionReply { engine: "Tinymist LSP", items });
        }
    }
    pub fn render(&self, req: RenderRequest) -> Result<Value, String> {
        let bin = self.bin.as_ref().map_err(Clone::clone)?;
        // Temporary .typ source is removed after compilation. It is never an editor format.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = self.root.join("target").join(format!("preview-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        struct Cleanup(PathBuf); impl Drop for Cleanup { fn drop(&mut self) { let _ = fs::remove_file(self.0.join("formula.typ")); let _ = fs::remove_file(self.0.join("formula.svg")); let _ = fs::remove_dir(&self.0); } }
        let _cleanup = Cleanup(dir.clone());
        let space = if req.display { " " } else { "" };
        let source = format!("#set page(width: auto, height: auto, margin: 0pt)\n#set text(font: \"New Computer Modern Math\", size: 24pt)\n{}\n${space}{}{space}$", req.definitions, req.expression);
        let file = dir.join("formula.typ"); fs::write(&file, source).map_err(|e| e.to_string())?;
        let svg_file = dir.join("formula.svg");
        let mut child = hidden(Command::new(bin).arg("compile").arg(&file).arg(&svg_file).args(["--format", "svg", "--ignore-system-fonts"]).arg("--root").arg(&dir)).current_dir(&dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
        let stdout = child.stdout.take().unwrap(); let stderr = child.stderr.take().unwrap();
        let output = std::thread::spawn(move || { let mut bytes = vec![]; let _ = stdout.take(16*1024*1024).read_to_end(&mut bytes); bytes });
        let errors = std::thread::spawn(move || { let mut bytes = vec![]; let _ = stderr.take(1024*1024).read_to_end(&mut bytes); bytes });
        let until = Instant::now() + Duration::from_secs(12);
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? { break status; }
            if Instant::now() > until { let _ = child.kill(); let _ = child.wait(); let _ = output.join(); let _ = errors.join(); return Err("Typst 渲染超时；源码已保留".into()); }
            std::thread::sleep(Duration::from_millis(20));
        };
        let _ = output.join().map_err(|_| "无法读取编译输出")?;
        let error = String::from_utf8_lossy(&errors.join().unwrap_or_default()).to_string();
        if !status.success() { return Err(error); }
        let svg = fs::read_to_string(svg_file).map_err(|e| e.to_string())?;
        if !svg.contains("<svg") { return Err("Typst 未返回 SVG".into()); }
        Ok(json!({"svg":svg,"engine":"Typst via Tinymist"}))
    }
}
fn position(source: &str, byte: usize) -> Value {
    let before = &source[..byte];
    json!({"line":before.bytes().filter(|b| *b == b'\n').count(),"character":before.rsplit('\n').next().unwrap_or("").encode_utf16().count()})
}
fn snippet_text(source: &str) -> Option<(String, usize)> {
    let mut result = String::new(); let mut caret = None; let mut final_caret = None;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => result.push(chars.next()?),
            '$' => {
                let braced = chars.peek() == Some(&'{'); if braced { chars.next(); }
                let mut number = String::new(); while chars.peek().is_some_and(char::is_ascii_digit) { number.push(chars.next()?); }
                if number.is_empty() { return None; }
                if number == "0" { final_caret = Some(result.len()); } else { caret.get_or_insert(result.len()); }
                if braced {
                    match chars.next()? {
                        '}' => {},
                        ':' => { loop { let c = chars.next()?; if c == '}' { break; } if c == '$' || c == '{' { return None; } result.push(c); } },
                        _ => return None,
                    }
                }
            }
            _ => result.push(c),
        }
    }
    let caret = caret.or(final_caret).unwrap_or(result.len()); Some((result, caret))
}
fn closing_brackets(source: &str) -> String {
    let mut stack = vec![]; let mut quoted = false; let mut escaped = false;
    for c in source.chars() {
        if escaped { escaped=false; continue; }
        if c == '\\' { escaped=true; continue; }
        if c == '"' { quoted=!quoted; continue; }
        if quoted { continue; }
        match c { '(' => stack.push(')'), '[' => stack.push(']'), '{' => stack.push('}'), ')' | ']' | '}' if stack.last()==Some(&c) => {stack.pop();}, _ => {} }
    }
    let mut suffix = if quoted { "\"".to_string() } else { String::new() };
    suffix.extend(stack.into_iter().rev()); suffix
}
fn offset(source: &str, position: &Value) -> Option<usize> {
    let line = position["line"].as_u64()? as usize; let character = position["character"].as_u64()? as usize;
    let start = if line == 0 { 0 } else { source.match_indices('\n').nth(line-1)?.0+1 };
    let mut units = 0;
    for (idx, ch) in source[start..].char_indices() {
        if units == character { return Some(start+idx); }
        if ch == '\n' { return None; }
        units += ch.len_utf16(); if units > character { return None; }
    }
    (units == character).then_some(source.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_positions_roundtrip_across_lines_and_non_bmp_characters() {
        let source="α𝑥\nbeta";
        for p in source.char_indices().map(|(p,_)| p).chain([source.len()]) { assert_eq!(offset(source,&position(source,p)),Some(p)); }
        assert_eq!(offset(source,&json!({"line":0,"character":2})),None);
    }
    #[test]
    fn tinymist_snippets_keep_argument_caret_without_committing_placeholders() {
        assert_eq!(snippet_text("alpha"),Some(("alpha".into(),5)));
        assert_eq!(snippet_text("cases(${1:})${0}"),Some(("cases()".into(),6)));
        assert_eq!(snippet_text("${1:x} + $0"),Some(("x + ".into(),0)));
        assert!(snippet_text("${UNKNOWN}").is_none());
    }
}
