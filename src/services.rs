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

struct DocumentLsp { lsp: Lsp, path: String, source: String, version: i64 }
impl Services {
    /// The native process is reused for a real file, with monotonically versioned full sync.
    pub fn language(&self, req: Value) -> Result<Value, String> {
        let path = req["path"].as_str().unwrap_or("main.typ");
        let file = crate::workspace::resolve(&self.workspace, path)?;
        let source = req["source"].as_str().ok_or("缺少文档源码")?;
        let method = req["method"].as_str().unwrap_or("diagnostics");
        if !["diagnostics", "completion", "hover", "definition", "formatting", "semanticTokens/full"].contains(&method) { return Err("不支持的 LSP 方法".into()); }
        let mut guard = self.document_lsp.lock().map_err(|e|e.to_string())?;
        let result = (|| {
            if guard.as_ref().is_none_or(|s|s.path != path) {
                let mut lsp = Lsp::start(self.bin.as_ref().map_err(Clone::clone)?, &self.workspace)?;
                lsp.uri = url::Url::from_file_path(&file).map_err(|_|"无效文件 URI")?.into();
                lsp.notify("textDocument/didOpen", json!({"textDocument":{"uri":lsp.uri,"languageId":"typst","version":1,"text":source}}))?;
                lsp.request("workspace/executeCommand", json!({"command":"tinymist.pinMain","arguments":[file]}))?;
                *guard = Some(DocumentLsp { lsp, path:path.into(), source:source.into(), version:1 });
            }
            let session = guard.as_mut().unwrap();
            if session.source != source {
                session.version += 1; session.source = source.into();
                session.lsp.diagnostics = json!([]); session.lsp.diagnostic_version = None;
                session.lsp.notify("textDocument/didChange",json!({"textDocument":{"uri":session.lsp.uri,"version":session.version},"contentChanges":[{"text":source}]}))?;
            }
            let lsp_method = if method == "diagnostics" { "hover" } else { method };
            let mut params = json!({"textDocument":{"uri":session.lsp.uri},"position":req.get("position").cloned().unwrap_or(json!({"line":0,"character":0}))});
            if method == "formatting" { params["options"] = json!({"tabSize":2,"insertSpaces":true}); }
            if method == "completion" { params["context"] = json!({"triggerKind":1}); }
            let value = session.lsp.request(&format!("textDocument/{lsp_method}"), params)?;
            if method == "diagnostics" {
                // Push diagnostics may arrive after the hover response. Bound the wait.
                let until = Instant::now() + Duration::from_millis(350);
                while let Ok(value) = session.lsp.messages.recv_timeout(until.saturating_duration_since(Instant::now())) {
                    if value["method"] == "textDocument/publishDiagnostics" && value["params"]["uri"] == session.lsp.uri {
                        session.lsp.diagnostics = value["params"]["diagnostics"].clone();
                        session.lsp.diagnostic_version = value["params"]["version"].as_i64();
                    } else if value.get("id").is_some() && value.get("method").is_some() {
                        let result = if value["method"] == "workspace/configuration" { json!([]) } else { Value::Null };
                        session.lsp.write(json!({"jsonrpc":"2.0","id":value["id"],"result":result}))?;
                    }
                    if Instant::now() >= until { break; }
                }
            }
            let diagnostics = if session.lsp.diagnostic_version.is_none_or(|v|v == session.version) { session.lsp.diagnostics.clone() } else { json!([]) };
            let root_uri=url::Url::from_directory_path(&self.workspace).map_err(|_|"无效项目目录")?;
            Ok(json!({"result":value,"legend":session.lsp.semantic_legend,"diagnostics":diagnostics,"version":session.version,"uri":session.lsp.uri,"rootUri":root_uri.as_str()}))
        })();
        if result.is_err() { *guard = None; }
        result
    }
}

pub(crate) struct Lsp { child: Child, input: ChildStdin, messages: mpsc::Receiver<Value>, next_id: u64, uri: String, diagnostics: Value, diagnostic_version: Option<i64>, semantic_legend: Value }
impl Drop for Lsp { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }
impl Lsp {
    pub(crate) fn start(bin: &Path, root: &Path) -> Result<Self, String> {
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
        let mut lsp = Self { child, input, messages, next_id: 0, uri, diagnostics: json!([]), diagnostic_version: None, semantic_legend: Value::Null };
        let initialized=lsp.request("initialize", json!({"processId":std::process::id(),"rootUri":url.as_str(),"capabilities":{"general":{"positionEncodings":["utf-16"]},"textDocument":{"completion":{"completionItem":{"snippetSupport":false}},"semanticTokens":{"requests":{"full":true},"tokenTypes":["comment","string","keyword","operator","number","function","variable","type"],"tokenModifiers":[],"formats":["relative"]}}},"initializationOptions":{"exportPdf":"never","semanticTokens":"enable","fontPaths":[]}}))?;
        lsp.semantic_legend=initialized["capabilities"]["semanticTokensProvider"]["legend"].clone();
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
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let value = self.messages.recv_timeout(until.saturating_duration_since(Instant::now())).map_err(|_| "Tinymist 响应超时".to_string())?;
            if value.get("method").is_some() && value.get("id").is_some() {
                let result = if value["method"] == "workspace/configuration" { json!([]) } else { Value::Null };
                self.write(json!({"jsonrpc":"2.0","id":value["id"],"result":result}))?;
            } else if value["method"] == "textDocument/publishDiagnostics" {
                if value["params"]["uri"] == self.uri { self.diagnostics = value["params"]["diagnostics"].clone(); self.diagnostic_version = value["params"]["version"].as_i64(); }
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
pub struct CompletionReply { pub engine: &'static str, pub items: Vec<visual_typst_core::cursor::CommandCompletion> }
#[derive(Deserialize, Serialize)]
pub struct RenderRequest { #[serde(default)] pub preview: bool, #[serde(default)] pub pdf:bool, #[serde(default)] pub overlays: std::collections::HashMap<String,String>, #[serde(default="default_path")] pub path: String, pub source: String, pub raw: Vec<RawRange>, #[serde(default)] pub formulas: Vec<RawRange>, #[serde(default)] pub preview_hashes:Vec<String>,
    /// Render the requested fragments on a source cut after this byte offset.
    ///
    /// Fragments are images taken out of a full document compile, so one mistake
    /// anywhere in the document leaves every one of them without an image. This
    /// value says how far the requested fragments reach; the source is then cut
    /// after the outermost node that holds them.
    #[serde(default)] pub context_end: Option<usize> }
#[derive(Deserialize, Serialize)]
pub struct RawRange { pub id: String, pub start: usize, pub end: usize }

/// Cut the source after the top-level node the fragments were asked for.
///
/// The prefix stays byte-identical, so the fragment ranges keep their meaning and
/// the formula keeps its enclosing containers; everything after the cut is dropped.
/// Typst never styles backwards, so nothing that is dropped can change these
/// fragments, and an error further down the document can no longer take them with
/// it. Cutting at a node boundary (rather than closing open delimiters by hand)
/// keeps the result valid Typst whatever the prefix contains.
fn context_source(source: &str, end: usize) -> Result<String,String> {
    if end == 0 || end > source.len() || !source.is_char_boundary(end) { return Err("取图上下文位置无效".into()); }
    let parsed = typst_syntax::Source::detached(source);
    let root = typst_syntax::LinkedNode::new(parsed.root());
    let mut cut = end;
    for node in root.children() {
        if node.offset() <= end && end <= node.offset() + node.len() { cut = node.offset() + node.len(); break; }
    }
    Ok(source[..cut].to_owned())
}

struct RenderAdapter { child: Child, requests: mpsc::Sender<Vec<u8>>, replies: mpsc::Receiver<Result<Value,String>> }
impl Drop for RenderAdapter { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }
impl RenderAdapter {
    pub(crate) fn start(bin: &Path, root: &Path) -> Result<Self,String> {
        let mut child = hidden(Command::new(bin).arg("--server")).current_dir(root)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
            .map_err(|e|format!("请运行 build-desktop.cmd 构建实时 Typst 引擎：{e}"))?;
        let mut input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (requests, receiver) = mpsc::channel::<Vec<u8>>();
        let (sender, replies) = mpsc::channel();
        std::thread::spawn(move || { for body in receiver { if input.write_all(&body).and_then(|_|input.write_all(b"\n")).and_then(|_|input.flush()).is_err() { break; } } });
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.by_ref().take(64*1024*1024+1).read_line(&mut line) { Ok(0) | Err(_) => break, _ => {} }
                if line.len() > 64*1024*1024 { let _ = sender.send(Err("Typst SVG 响应过大".into())); break; }
                if sender.send(serde_json::from_str(&line).map_err(|e|e.to_string())).is_err() { break; }
            }
        });
        Ok(Self { child,requests,replies })
    }
    fn request(&mut self, req: &RenderRequest) -> Result<Value,String> {
        self.requests.send(serde_json::to_vec(req).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let value = self.replies.recv_timeout(Duration::from_secs(30)).map_err(|_|"Typst 实时编译超时；源码已保留")??;
        // A document diagnostic is a valid protocol reply; keep the warm engine.
        Ok(value)
    }
}

#[derive(Deserialize, Serialize)]
pub struct AttachmentRequest { #[serde(default="default_path")] pub path: String, pub expression: String, #[serde(default)] pub definitions: String, pub display: bool }

pub struct Services { bin: Result<PathBuf, String>, root: PathBuf, completion_lock: Mutex<()>, document_lsp: Mutex<Option<DocumentLsp>>, pub workspace: PathBuf, render_adapter: Mutex<Option<RenderAdapter>> }
impl Services {
    pub fn new(root: PathBuf) -> Self { Self { bin: find_tinymist(), workspace: std::env::var_os("VISUAL_TYPST_WORKSPACE").map(PathBuf::from).unwrap_or_else(|| root.join("workspace")), root, document_lsp: Mutex::new(None), completion_lock: Mutex::new(()), render_adapter: Mutex::new(None) } }
    fn adapter_bin(&self) -> PathBuf {
        if let Some(path)=std::env::var_os("VISUAL_TYPST_ADAPTER") { return path.into(); }
        let name=if cfg!(windows) { "visual-typst-layout.exe" } else { "visual-typst-layout" };
        let packaged=self.root.join(name); if packaged.is_file() { return packaged; }
        self.root.join("target/adapter").join(if cfg!(debug_assertions) { "debug" } else { "release" }).join(if cfg!(windows) { "visual-typst-layout.exe" } else { "visual-typst-layout" }) }
    pub fn status(&self) -> Value { match &self.bin { Ok(path) => json!({"available":true,"attachments":self.adapter_bin().is_file(),"engine":"Tinymist LSP + Typst","path":path}), Err(error) => json!({"available":false,"attachments":self.adapter_bin().is_file(),"error":error}) } }
    pub fn attachments(&self, req: AttachmentRequest) -> Result<Value, String> {
        crate::workspace::resolve(&self.workspace, &req.path)?;
        if !self.adapter_bin().is_file() { return Err("请运行 build-desktop.cmd 并重启服务，以启用 Typst limits/stretch 适配器".into()); }
        let body = serde_json::to_vec(&req).map_err(|e| e.to_string())?;
        let mut child = hidden(&mut Command::new(self.adapter_bin())).current_dir(&self.workspace)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
        let mut input = child.stdin.take().unwrap();
        // Put writes on a separate thread so even a stuck child cannot block
        // the request timeout. Dropping stdin tells the adapter the request ends.
        let writer = std::thread::spawn(move || input.write_all(&body));
        let stdout = child.stdout.take().unwrap(); let stderr = child.stderr.take().unwrap();
        let output = std::thread::spawn(move || { let mut b = vec![]; let _ = stdout.take(64*1024*1024).read_to_end(&mut b); b });
        let errors = std::thread::spawn(move || { let mut b = vec![]; let _ = stderr.take(1024*1024).read_to_end(&mut b); b });
        let until = Instant::now() + Duration::from_secs(30);
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
        // The URI is built by this module, so a failure here is a bug, not input.
        let pin = url::Url::parse(&lsp.uri).ok().and_then(|url| url.to_file_path().ok()).ok_or("内部 LSP 文件 URI 无效")?;
        lsp.request("workspace/executeCommand", json!({"command":"tinymist.pinMain","arguments":[pin]}))?;
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
                items.push(visual_typst_core::cursor::CommandCompletion { label: label.into(), replacement, caret: start-req.start+cursor });
            }
        }
        let prefix = req.source[req.start..req.caret].rsplit(|c:char| !c.is_alphanumeric() && c != '_' && c != '-').next().unwrap_or("");
        items.sort_by_key(|i| (!i.label.starts_with(prefix), i.label.clone()));
        items.dedup_by(|a,b| a.label == b.label);
        if items.is_empty() && attempt < 5 { attempt += 1; std::thread::sleep(Duration::from_millis(60)); continue; }
        return Ok(CompletionReply { engine: "Tinymist LSP", items });
        }
    }
    pub fn render(&self, mut req: RenderRequest) -> Result<Value, String> {
        crate::workspace::resolve(&self.workspace, &req.path)?;
        if let Some(end) = req.context_end.take() {
            req.source = context_source(&req.source, end)?;
            // A range that the cut removed cannot be rendered, and the cut is only
            // ever chosen at the end of a node that holds these ranges.
            if req.raw.iter().chain(req.formulas.iter()).any(|range| range.end > req.source.len()) { return Err("取图区间不在编译上下文内".into()); }
        }
        let mut adapter = self.render_adapter.lock().map_err(|e| e.to_string())?;
        if adapter.is_none() { *adapter = Some(RenderAdapter::start(&self.adapter_bin(), &self.workspace)?); }
        let result = adapter.as_mut().unwrap().request(&req);
        if result.is_err() { *adapter = None; }
        let value = result?;
        if let Some(error) = value["error"].as_str() { return Err(error.into()); }
        Ok(value)
    }
}
pub fn position(source: &str, byte: usize) -> Value {
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
pub fn offset(source: &str, position: &Value) -> Option<usize> {
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
    #[test]
    fn a_fragment_context_ends_at_the_node_that_holds_it() {
        let source="#set text(size: 11pt)\n\n正文 $ sum_(n=1)^oo x^n $ 之后\n\n#panic(\"坏了\")\n";
        let formula=source.find("$ sum").unwrap();
        // A top-level formula is its own context: everything after it is dropped.
        let cut=context_source(source,source.find(" $ 之后").unwrap()).unwrap();
        assert_eq!(cut,"#set text(size: 11pt)\n\n正文 $ sum_(n=1)^oo x^n $");
        assert!(cut.len() > formula);
        // Inside a container the whole container is kept, so the formula keeps the
        // styles it is laid out with.
        let block="#set page(width: 10cm)\n#block(fill: luma(90%))[\n  #set text(size: 20pt)\n  $a+b$ 后面\n]\n#panic(\"坏了\")\n";
        let cut=context_source(block,block.find("$a+b$").unwrap()+4).unwrap();
        assert!(cut.starts_with("#set page(width: 10cm)\n#block"));
        assert!(cut.ends_with("]"),"{cut:?}");
        assert!(!cut.contains("panic"),"what follows the container cannot affect it");
        // Offsets inside the prefix keep their meaning, which is what lets the
        // requested ranges be used unchanged.
        assert_eq!(context_source(source,source.len()).unwrap(),source);
        for bad in [0,source.len()+1] { assert!(context_source(source,bad).is_err()); }
    }
}


fn default_path() -> String { "main.typ".into() }
