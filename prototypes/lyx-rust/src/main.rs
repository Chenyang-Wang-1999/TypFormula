// SPDX-License-Identifier: GPL-2.0-or-later
// Local assets plus native Tinymist services; editing stays in Rust/WASM.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::{io::Read, sync::Arc};
    use tiny_http::{Header, Method, Response, Server};
    use lyx_typst_core::services::Services;
    let port = std::env::args().nth(1).unwrap_or_else(|| "4320".into()).parse::<u16>()?;
    let server = Server::http(("127.0.0.1", port))?;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let services = Arc::new(Services::new(root.to_path_buf()));
    println!("LyX → Rust / Typst · http://127.0.0.1:{port}\n{}\nCtrl+C 停止", services.status());
    for mut request in server.incoming_requests() {
        let url = request.url().split('?').next().unwrap_or("").to_owned();
        let header = |name: &str| request.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str());
        let host_ok = header("Host").is_some_and(|s| s == format!("localhost:{port}") || s == format!("127.0.0.1:{port}"));
        let origin_ok = header("Origin").is_none_or(|s| s == format!("http://localhost:{port}") || s == format!("http://127.0.0.1:{port}"));
        let json_ok = request.method() != &Method::Post || header("Content-Type").is_some_and(|s| s.starts_with("application/json"));
        if !host_ok || !origin_ok || !json_ok { request.respond(Response::from_string("Forbidden").with_status_code(403))?; continue; }
        if url.starts_with("/api/") {
            let services = services.clone();
            std::thread::spawn(move || {
                let result: Result<serde_json::Value, String> = (|| {
                    if url == "/api/status" && request.method() == &Method::Get { return Ok(services.status()); }
                    if request.method() != &Method::Post { return Err("需要 POST 请求".into()); }
                    let mut body = String::new(); request.as_reader().take(256*1024+1).read_to_string(&mut body).map_err(|e| e.to_string())?;
                    if body.len() > 256*1024 { return Err("请求过大".into()); }
                    match url.as_str() {
                        "/api/completion" => serde_json::to_value(services.complete(serde_json::from_str(&body).map_err(|e| e.to_string())?)?).map_err(|e| e.to_string()),
                        "/api/render" => services.render(serde_json::from_str(&body).map_err(|e| e.to_string())?),
                        "/api/attachments" => services.attachments(serde_json::from_str(&body).map_err(|e| e.to_string())?),
                        _ => Err("未知请求".into()),
                    }
                })();
                let (status, body) = match result { Ok(v) => (200, v), Err(error) => (422, serde_json::json!({"error":error})) };
                let _ = request.respond(Response::from_string(body.to_string()).with_status_code(status).with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap()));
            });
            continue;
        }
        let asset = match url.as_str() {
            "/" => Some(("index.html", "text/html; charset=utf-8")),
            "/app.js" => Some(("app.js", "text/javascript; charset=utf-8")),
            "/math-font.js" => Some(("math-font.js", "text/javascript; charset=utf-8")),
            "/style.css" => Some(("style.css", "text/css; charset=utf-8")),
            "/core.wasm" => Some(("core.wasm", "application/wasm")),
            "/fonts/NewCMMath-Regular.otf" => Some(("fonts/NewCMMath-Regular.otf", "font/otf")),
            "/fonts/NewCM10-Italic.otf" => Some(("fonts/NewCM10-Italic.otf", "font/otf")),
            _ => None,
        };
        if let Some((file, mime)) = asset {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web").join(file);
            match std::fs::read(path) {
                Ok(bytes) => { request.respond(Response::from_data(bytes)
                    .with_header(Header::from_bytes("Content-Type", mime).unwrap())
                    .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
                    .with_header(Header::from_bytes("Content-Security-Policy", "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' blob:; object-src 'none'; frame-ancestors 'none'").unwrap()))?; }
                Err(_) => { request.respond(Response::from_string("请先运行 build.cmd 构建 Rust/WASM").with_status_code(503))?; }
            }
        } else { request.respond(Response::from_string("Not found").with_status_code(404))?; }
    }
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
