//! Private stdio transport for the desktop window. No listening socket.
use crate::{packages, services::Services};
use serde_json::{Value, json};
use std::{io::{self, BufRead, Read, Write}, path::PathBuf, sync::{Arc, Mutex}};

pub fn serve(workspace: PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let exe = std::env::current_exe()?;
    let mut services = Services::new(exe.parent().unwrap().to_path_buf());
    services.workspace = workspace.canonicalize()?;
    // Requests are answered by id, so they run on their own threads exactly like
    // the HTTP frontend: a long page preview must not queue the formula renders
    // and language requests behind it. Services is shared the same way there.
    let services = Arc::new(services);
    let output = Arc::new(Mutex::new(io::stdout()));
    let stdin = io::stdin(); let mut reader = stdin.lock();
    let mut workers = vec![];
    loop {
        let mut line = String::new();
        if reader.by_ref().take(8*1024*1024+1).read_line(&mut line)? == 0 { break; }
        if line.len() > 8*1024*1024 { break; }
        let request: Value = serde_json::from_str(&line)?;
        let services = services.clone(); let output = output.clone();
        workers.push(std::thread::spawn(move || {
            let result = dispatch(&services, &request);
            let reply = match result { Ok(result)=>json!({"id":request["id"],"result":result}), Err(error)=>json!({"id":request["id"],"error":error}) };
            // Whole lines only: two replies must never interleave.
            if let Ok(mut output) = output.lock() { let _ = writeln!(output,"{reply}"); let _ = output.flush(); }
        }));
    }
    for worker in workers { let _ = worker.join(); }
    Ok(())
}
pub fn dispatch(services: &Services, request: &Value) -> Result<Value,String> {
    let body=request["body"].clone();
    match request["route"].as_str().unwrap_or("") {
        "/api/status"=>Ok(services.status()),
        "/api/render"|"/api/preview"|"/api/pdf"=>services.render(serde_json::from_value(body).map_err(|e|e.to_string())?),
        "/api/attachments"=>services.attachments(serde_json::from_value(body).map_err(|e|e.to_string())?),
        "/api/glyphs"=>services.glyphs(body),
        "/api/completion"=>serde_json::to_value(services.complete(serde_json::from_value(body).map_err(|e|e.to_string())?)?).map_err(|e|e.to_string()),
        "/api/lsp"=>services.language(body),
        "/api/preview/live"=>services.preview(body),
        "/api/packages"=>packages::handle(body),
        _=>Err("未知后端请求".into()),
    }
}
