use multi_formula_context::{engine::Backend, Error, NodeId};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Read, path::{Path, PathBuf}, sync::Arc};
use tiny_http::{Header, Method, Request, Response, Server};
use typst_syntax::{RootedPath, Source, VirtualPath, VirtualRoot};

#[derive(Deserialize)]
struct Update { revision: u64, source: String }
#[derive(Deserialize)]
struct Edit { revision: u64, start: usize, end: usize, text: String }
#[derive(Deserialize)]
struct Fragment { node: NodeId, container: Option<NodeId> }
#[derive(Deserialize)]
struct Page { revision: u64, render_generation: u64, page: usize }
#[derive(Deserialize)]
struct NodeRequest { node: NodeId }
#[derive(Deserialize)]
struct CachedSvg { since: Option<u64> }
#[derive(Deserialize)]
struct RefreshSvg {
    revision: u64,
    #[serde(default)] formulas: Vec<NodeId>,
    #[serde(default)] nodes: Vec<NodeId>,
    #[serde(default)] all: bool,
    #[serde(default)] dependencies: bool,
}

fn respond_api(mut request: Request, url: &str, backend: &Backend) {
    let result: Result<Value, Error> = (|| {
        if request.method() == &Method::Get {
            return match url {
                "/api/document" => Ok(backend.document_json()),
                "/api/state" => Ok(backend.state_json()),
                "/api/svg" => Ok(backend.svg_json(None)),
                _ => Err(Error("未知请求".into())),
            };
        }
        if request.method() != &Method::Post { return Err(Error("需要 POST 请求".into())); }
        let mut body = String::new();
        request.as_reader().take(512 * 1024 + 1).read_to_string(&mut body).map_err(|e| Error(e.to_string()))?;
        if body.len() > 512 * 1024 { return Err(Error("请求过大".into())); }
        fn parse<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, Error> {
            serde_json::from_str(body).map_err(|e| Error(e.to_string()))
        }
        match url {
            "/api/document" => {
                let update: Update = parse(&body)?;
                {
                    let mut document = backend.document.lock().unwrap();
                    if update.revision != document.revision() { return Err(Error("文档版本冲突，请先重新载入".into())); }
                    document.replace(&update.source);
                }
                backend.sync_svg_targets();
                backend.schedule();
                Ok(backend.document_json())
            }
            "/api/edit" => {
                let edit: Edit = parse(&body)?;
                backend.document.lock().unwrap().edit(edit.revision, edit.start..edit.end, &edit.text)?;
                backend.sync_svg_targets();
                backend.schedule();
                Ok(backend.document_json())
            }
            "/api/fragment" => {
                let fragment: Fragment = parse(&body)?;
                backend.fragment(fragment.node, fragment.container)
            }
            "/api/svg" => {
                let read: CachedSvg = parse(&body)?;
                Ok(backend.svg_json(read.since))
            }
            "/api/page" => {
                let page: Page = parse(&body)?;
                backend.page(page.revision, page.render_generation, page.page)
            }
            "/api/node" => {
                let node: NodeRequest = parse(&body)?;
                let document = backend.document.lock().unwrap();
                Ok(json!({"source": document.text(node.node)?, "range": document.node(node.node)?.range}))
            }
            "/api/refresh" => {
                backend.refresh_files();
                Ok(json!({"scheduled": true}))
            }
            "/api/refresh-svg" => {
                let refresh: RefreshSvg = parse(&body)?;
                backend.refresh_svg(refresh.revision, &refresh.formulas, &refresh.nodes, refresh.all, refresh.dependencies)
            }
            _ => Err(Error("未知请求".into())),
        }
    })();
    let (status, body) = match result {
        Ok(value) => (200, value),
        Err(error) => (422, json!({"error": error.to_string()})),
    };
    let _ = request.respond(Response::from_string(body.to_string()).with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap()));
}

fn asset(url: &str, root: &Path) -> Option<(PathBuf, &'static str)> {
    if url == "/fonts/NewCMMath-Regular.otf" {
        return Some((root.parent()?.join("shared/fonts/NewCMMath-Regular.otf"), "font/otf"));
    }
    let (file, mime) = match url {
        "/" => ("index.html", "text/html; charset=utf-8"),
        "/document.js" => ("document.js", "text/javascript; charset=utf-8"),
        "/editor.js" => ("editor.js", "text/javascript; charset=utf-8"),
        "/math-font.js" => ("math-font.js", "text/javascript; charset=utf-8"),
        "/document.css" => ("document.css", "text/css; charset=utf-8"),
        "/document-editor.css" => ("document-editor.css", "text/css; charset=utf-8"),
        _ => return None,
    };
    Some((root.join("web").join(file), mime))
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args().skip(1);
    let port = args.next().unwrap_or_else(|| "4321".into()).parse::<u16>()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_file = args.next().map(PathBuf::from);
    let (source, project) = if let Some(path) = source_file {
        let path = path.canonicalize()?;
        let name = path.file_name().unwrap().to_string_lossy();
        let id = RootedPath::new(VirtualRoot::Project, VirtualPath::new(&name)?).intern();
        (Source::new(id, std::fs::read_to_string(&path)?), path.parent().unwrap().to_owned())
    } else {
        (Source::detached("这是一个 Typst 文档。可以在光标处插入宏或公式。\n\n"), root.to_owned())
    };
    let backend = Backend::new(source, project)?;
    let server = Server::http(("127.0.0.1", port))?;
    println!("Typst 文档与公式引擎 · http://127.0.0.1:{port}\nCtrl+C 停止；编辑保存在当前会话，可复制源码保存。");
    for request in server.incoming_requests() {
        let url = request.url().split('?').next().unwrap_or("").to_owned();
        let header = |name: &str| request.headers().iter()
            .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str());
        let host_ok = header("Host").is_some_and(|host| host == format!("localhost:{port}") || host == format!("127.0.0.1:{port}"));
        let origin_ok = header("Origin").is_none_or(|origin| origin == format!("http://localhost:{port}") || origin == format!("http://127.0.0.1:{port}"));
        let json_ok = request.method() != &Method::Post || header("Content-Type").is_some_and(|v| v.starts_with("application/json"));
        if !host_ok || !origin_ok || !json_ok {
            request.respond(Response::from_string("Forbidden").with_status_code(403))?;
            continue;
        }
        if url.starts_with("/api/") {
            let backend = Arc::clone(&backend);
            std::thread::spawn(move || respond_api(request, &url, &backend));
        } else if let Some((path, mime)) = asset(&url, root) {
            match std::fs::read(path) {
                Ok(bytes) => request.respond(Response::from_data(bytes)
                    .with_header(Header::from_bytes("Content-Type", mime).unwrap())
                    .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
                    .with_header(Header::from_bytes("Content-Security-Policy",
                        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob:; object-src 'none'; frame-ancestors 'none'").unwrap()))?,
                Err(_) => request.respond(Response::from_string("缺少页面资源").with_status_code(404))?,
            }
        } else { request.respond(Response::from_string("Not found").with_status_code(404))?; }
    }
    Ok(())
}
