@echo off
rem Double-click to try JuiceBox Trainer: opens the fake test game in one window
rem and the JuiceBox Trainer app (development build) in another.
rem Needs Rust (rustup) and Node.js 22+ installed. The first start takes a few
rem minutes while everything is built; later starts are quick.

setlocal
cd /d "%~dp0"
title JuiceBox Trainer launcher

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust is not installed. Get it from https://rustup.rs and try again.
  pause
  exit /b 1
)
where npm >nul 2>nul
if errorlevel 1 (
  echo Node.js is not installed. Get version 22 or newer from https://nodejs.org and try again.
  pause
  exit /b 1
)

rem Install when packages are missing or older than this version (the AI
rem helper's package arrived in 0.8.0).
if not exist node_modules\@anthropic-ai\claude-agent-sdk\package.json (
  echo Installing the app's packages. This happens once and can take a few minutes...
  call npm ci
  if errorlevel 1 (
    echo Installing packages failed.
    pause
    exit /b 1
  )
)

powershell -NoProfile -ExecutionPolicy Bypass -File scripts\try-prepare.ps1
if errorlevel 2 (
  echo Opening only the fake game in a new window.
  start "JuiceBox Trainer testbed (fake game)" cmd /k cargo run -q -p testbed-native --target-dir target\try
  timeout /t 5 >nul
  exit /b 0
)
if errorlevel 1 (
  pause
  exit /b 1
)

echo Opening the fake game (testbed) in its own window...
rem Its own build folder, so an older testbed that is still open doesn't block it.
start "JuiceBox Trainer testbed (fake game)" cmd /k cargo run -q -p testbed-native --target-dir target\try

echo Preparing the AI helper...
call npm run build:agent >nul
if errorlevel 1 echo The AI helper could not be built; the rest of JuiceBox Trainer still works.

echo Starting JuiceBox Trainer. The app window opens when it's ready.
echo Close this window to stop JuiceBox Trainer.
call npm run dev
if errorlevel 1 (
  echo.
  echo JuiceBox Trainer stopped with an error. The message above says why.
  pause
  exit /b 1
)
