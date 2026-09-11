// SPDX-License-Identifier: GPL-2.0-or-later
//! Process entry points for the native desktop editor.
//!
//! `--desktop-core` is one document plus one formula session over stdin/stdout;
//! `--stdio <workspace>` is the Tinymist/Typst service pipe the window spawns
//! beside it. There is no listening socket: every client of this binary is the
//! window that started it, which is also why the two modes are private pipes.
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match std::env::args().nth(1).as_deref() {
        Some("--desktop-core") => visual_typst::desktop::serve(),
        Some("--stdio") => {
            let workspace = std::env::args_os().nth(2).map(std::path::PathBuf::from).ok_or("Missing workspace")?;
            visual_typst::rpc::serve(workspace)
        }
        _ => Err("用法：visual-typst --desktop-core | --stdio <工作目录>".into()),
    }
}
