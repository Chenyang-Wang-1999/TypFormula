@echo off
setlocal
cd /d "%~dp0"
call npm ci
if errorlevel 1 exit /b 1
call npm run build
if errorlevel 1 exit /b 1
cargo build --offline --locked --release --target wasm32-unknown-unknown --lib
if errorlevel 1 exit /b 1
copy /y target\wasm32-unknown-unknown\release\visual_typst_core.wasm web\core.wasm >nul
if errorlevel 1 exit /b 1
cargo build --locked --features server --bin visual-typst --target-dir target\server
if errorlevel 1 exit /b 1
cargo build --locked --manifest-path native-adapter\Cargo.toml --target-dir target\adapter
if errorlevel 1 exit /b 1
call npm run package:vscode
exit /b %errorlevel%
