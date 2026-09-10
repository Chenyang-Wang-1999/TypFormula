@echo off
setlocal
cd /d "%~dp0"
cargo build --offline --locked --release --features server --bin visual-typst --target-dir target/server
if errorlevel 1 exit /b 1
cargo build --offline --locked --release --manifest-path native-adapter/Cargo.toml --target-dir target/adapter
if errorlevel 1 exit /b 1
if defined VISUAL_TYPST_PYTHON (
  "%VISUAL_TYPST_PYTHON%" -c "from PyQt5 import QtWidgets, QtSvg; import desktop.window"
) else (
  python -c "from PyQt5 import QtWidgets, QtSvg; import desktop.window"
)
if errorlevel 1 (
  echo Install native UI dependencies: python -m pip install -r desktop/requirements.txt
  exit /b 1
)
echo Native desktop build ready. Run start-desktop.cmd to open the app.
