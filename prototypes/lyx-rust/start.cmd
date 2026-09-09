@echo off
cd /d "%~dp0"
if not exist web\core.wasm call build.cmd
if errorlevel 1 exit /b 1
call build-native.cmd
if errorlevel 1 exit /b 1
cargo run --locked -- %*
if errorlevel 1 pause
