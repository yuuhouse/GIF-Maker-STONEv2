@echo off
REM Rust GIF Maker - Run Script
REM This script builds and runs the GIF Maker application

setlocal enabledelayedexpansion

echo.
echo ====================================
echo  Rust GIF Maker - Build and Run
echo ====================================
echo.

REM Check if Rust is installed
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo [ERROR] Rust is not installed or not in PATH
    echo Please install Rust from https://rustup.rs/
    pause
    exit /b 1
)

REM Check if FFmpeg is installed
where ffmpeg >nul 2>nul
if %errorlevel% neq 0 (
    echo [WARNING] FFmpeg is not found in PATH
    echo The application may not work properly without FFmpeg
    echo Please install FFmpeg from https://ffmpeg.org/download.html
    echo.
)

echo [1/2] Building the project in release mode...
echo.
cargo build --release

if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Build failed!
    pause
    exit /b 1
)

echo.
echo [2/2] Running the application...
echo.
cargo run --release

pause
