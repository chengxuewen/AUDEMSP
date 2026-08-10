@echo off
REM bootstrap.bat — First-time setup for AUDEMSP development (Windows)
REM Usage: bootstrap.bat
REM After initial setup, use: pixi.bat

set "SCRIPT_DIR=%~dp0"

echo ================================================
echo   AUDEMSP Development Environment Bootstrap
echo ================================================
echo.

echo [1/2] Installing pixi and project dependencies...
call "%SCRIPT_DIR%scripts\pixi-init.bat"

echo.
echo [2/2] Activating pixi environment...
call "%SCRIPT_DIR%scripts\pixi-shell.bat"

echo.
echo ================================================
echo   AUDEMSP environment ready!
echo ================================================
echo.
echo Next time, just run: pixi.bat
echo CLI ready: audemsp.bat -h   (build/up/e2e/clean/config/status...)
