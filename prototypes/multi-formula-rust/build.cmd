@echo off
setlocal
cd /d "%~dp0"
if /i "%~1"=="--online" goto online
powershell -NoProfile -ExecutionPolicy Bypass -File prepare-engine.ps1
if errorlevel 1 exit /b 1
cargo build --offline --bin multi-formula-demo
if errorlevel 1 (
  echo Build failed. If dependencies are not cached, run build.cmd --online.
  exit /b 1
)
exit /b 0

:online
powershell -NoProfile -ExecutionPolicy Bypass -File prepare-engine.ps1 -Online
if errorlevel 1 exit /b 1
cargo build --bin multi-formula-demo
exit /b %errorlevel%
