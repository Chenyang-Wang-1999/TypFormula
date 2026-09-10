@echo off
setlocal
cd /d "%~dp0"
if defined VISUAL_TYPST_PYTHON (
  "%VISUAL_TYPST_PYTHON%" -m desktop %*
) else (
  pythonw -m desktop %*
)
