@echo off
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File native-adapter\prepare.ps1
if errorlevel 1 exit /b 1
cargo build --manifest-path native-adapter\Cargo.toml --target-dir target\adapter
if errorlevel 1 exit /b 1
