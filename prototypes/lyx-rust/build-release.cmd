@echo off
cd /d "%~dp0"
if errorlevel 1 exit /b 1

call build.cmd
if errorlevel 1 goto failed

rem target may have been deleted: restore the pinned Typst source and bridge.
powershell -NoProfile -ExecutionPolicy Bypass -File native-adapter\prepare.ps1
if errorlevel 1 goto failed

cargo build --locked --release --manifest-path native-adapter\Cargo.toml --target-dir target\adapter
if errorlevel 1 goto failed

cargo build --locked --release --bin lyx-typst-demo --target-dir target
if errorlevel 1 goto failed

if not exist target\release\lyx-typst-demo.exe goto missing
if not exist target\adapter\release\lyx-typst-layout-adapter.exe goto missing
echo Release build completed:
echo   %~dp0target\release\lyx-typst-demo.exe
echo   %~dp0target\adapter\release\lyx-typst-layout-adapter.exe
echo Run start-release.cmd to launch without building.
exit /b 0

:missing
echo Expected release executables were not found. Check the Cargo target configuration.
:failed
echo Release build failed. See the error above.
exit /b 1
