@echo off
REM pixi-shell.bat — Activate pixi environment (Windows)
REM Run this script to enter a pixi-activated shell

set "SCRIPT_DIR=%~dp0"
set "PROJECT_ROOT=%SCRIPT_DIR%.."
set "PIXI_BIN=%USERPROFILE%\.pixi\bin\pixi.exe"

echo Activating AUDEMSP pixi environment...
"%PIXI_BIN%" shell-hook --manifest-path "%PROJECT_ROOT%\pixi.toml" > "%TEMP%\pixi-hook.bat"
call "%TEMP%\pixi-hook.bat"
del "%TEMP%\pixi-hook.bat"
echo.
echo AUDEMSP environment active. Deactivate with: exit
