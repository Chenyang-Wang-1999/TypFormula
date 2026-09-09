@echo off
setlocal
cd /d "%~dp0"
if not exist web\core.wasm goto missing
if not exist web\editor.bundle.js goto missing
if not exist target\adapter\debug\visual-typst-layout.exe goto missing
if not exist target\server\debug\visual-typst.exe goto missing
if not "%~1"=="" set "VISUAL_TYPST_WORKSPACE=%~f1"
target\server\debug\visual-typst.exe 4321
exit /b %errorlevel%
:missing
echo Please run build.cmd first.
exit /b 1
