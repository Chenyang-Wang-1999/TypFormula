@echo off
setlocal
cd /d "%~dp0"
if /i "%~1"=="--online" goto online
powershell -NoProfile -ExecutionPolicy Bypass -File prepare-engine.ps1
if errorlevel 1 exit /b 1
cargo test --offline
exit /b %errorlevel%

:online
powershell -NoProfile -ExecutionPolicy Bypass -File prepare-engine.ps1 -Online
if errorlevel 1 exit /b 1
cargo test
exit /b %errorlevel%
