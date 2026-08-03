@echo off
REM pixi-init.bat — Windows bootstrap for AUDEMSP pixi environment
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
set "PROJECT_ROOT=%SCRIPT_DIR%.."
set "PIXI_VERSION=0.67.2"
set "PIXI_BIN=%USERPROFILE%\.pixi\bin\pixi.exe"

echo === AUDEMSP pixi environment setup (Windows) ===
echo Project root: %PROJECT_ROOT%

REM Check if pixi is installed
if not exist "%PIXI_BIN%" (
    echo Installing pixi %PIXI_VERSION%...
    powershell -Command "Invoke-WebRequest -Uri 'https://github.com/prefix-dev/pixi/releases/download/v%PIXI_VERSION%/pixi-x86_64-pc-windows-msvc.zip' -OutFile '%TEMP%\pixi.zip'"
    powershell -Command "Expand-Archive -Path '%TEMP%\pixi.zip' -DestinationPath '%USERPROFILE%\.pixi\bin' -Force"
    del "%TEMP%\pixi.zip"
)

REM Install project dependencies
echo Installing project dependencies...
cd /d "%PROJECT_ROOT%"
"%PIXI_BIN%" install --manifest-path "%PROJECT_ROOT%\pixi.toml"
if errorlevel 1 (
    echo pixi install failed. Regenerating lock file...
    "%PIXI_BIN%" update --manifest-path "%PROJECT_ROOT%\pixi.toml"
    "%PIXI_BIN%" install --manifest-path "%PROJECT_ROOT%\pixi.toml"
)

echo.
echo === AUDEMSP pixi environment ready ===
echo Activate with: pixi.bat
echo Or run tasks:  pixi run build ^| pixi run test ^| pixi run lint
