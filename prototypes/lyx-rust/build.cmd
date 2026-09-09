@echo off
cd /d "%~dp0"
cargo build --locked --release --target wasm32-unknown-unknown --lib
if errorlevel 1 exit /b 1
copy /y target\wasm32-unknown-unknown\release\lyx_typst_core.wasm web\core.wasm >nul
if errorlevel 1 exit /b 1
