@echo off
cd /d "%~dp0"
if errorlevel 1 exit /b 1
if not exist web\core.wasm goto missing
if not exist target\release\lyx-typst-demo.exe goto missing
if not exist target\adapter\release\lyx-typst-layout-adapter.exe goto missing
target\release\lyx-typst-demo.exe %*
exit /b %errorlevel%

:missing
echo Release files are missing. Run build-release.cmd first.
exit /b 1
