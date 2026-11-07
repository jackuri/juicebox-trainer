@echo off
rem Double-click to try OpenTrainer: opens the fake test game in one window
rem and the OpenTrainer app (development build) in another.
rem Needs Rust (rustup) and Node.js 22+ installed. The first start takes a few
rem minutes while everything is built; later starts are quick.

setlocal
cd /d "%~dp0"
title OpenTrainer launcher

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

if not exist node_modules (
  echo Installing the app's packages, once...
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
  start "OpenTrainer testbed (fake game)" cmd /k cargo run -q -p testbed-native --target-dir target\try
  timeout /t 5 >nul
  exit /b 0
)
if errorlevel 1 (
  pause
  exit /b 1
)

echo Opening the fake game (testbed) in its own window...
rem Its own build folder, so an older testbed that is still open doesn't block it.
start "OpenTrainer testbed (fake game)" cmd /k cargo run -q -p testbed-native --target-dir target\try

echo Starting OpenTrainer. The app window opens when it's ready.
echo Close this window to stop OpenTrainer.
call npm run dev
if errorlevel 1 (
  echo.
  echo OpenTrainer stopped with an error. The message above says why.
  pause
  exit /b 1
)
