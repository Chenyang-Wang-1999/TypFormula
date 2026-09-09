@echo off
setlocal
cd /d "%~dp0"
where node >nul 2>nul
if errorlevel 1 (
  echo Node.js is required for the optional frontend unit tests.
  exit /b 1
)
node --test tests/editor_state.mjs
exit /b %errorlevel%
