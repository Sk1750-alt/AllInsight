@echo off
REM ===================================================================
REM  AllInsight - production Windows build
REM
REM  Runs the type check, the Rust test suite, the frontend build and
REM  the Tauri bundler, then copies the artefacts into dist-release\.
REM  Stops at the first failure rather than shipping something broken.
REM ===================================================================

setlocal
cd /d "%~dp0"

echo.
echo  AllInsight - production build
echo  =========================
echo.

REM rustup installs into the user profile and the installer does not always
REM refresh PATH for an already-open console.
if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where cargo >nul 2>nul
if errorlevel 1 (
  echo  [!] Rust was not found on PATH.
  echo      Install it from https://rustup.rs and open a new terminal.
  exit /b 1
)

where node >nul 2>nul
if errorlevel 1 (
  echo  [!] Node.js was not found on PATH.
  echo      Install Node.js 20 or later from https://nodejs.org
  exit /b 1
)

if not exist "node_modules" (
  echo  [1/5] Installing dependencies...
  call npm install || exit /b 1
) else (
  echo  [1/5] Dependencies already installed.
)

echo.
echo  [2/5] Type-checking the interface...
call npm run typecheck || exit /b 1

echo.
echo  [3/5] Running the Rust test suite...
pushd src-tauri
cargo test || (popd & exit /b 1)
popd

echo.
echo  [4/5] Building the frontend...
call npm run build || exit /b 1

echo.
echo  [5/5] Building the application and installer...
REM Clear the previous bundle first. The collect step below copies *.exe, so an
REM installer left over from an earlier name or version would be copied forward
REM as though this build had produced it.
if exist "src-tauri\target\release\bundle\nsis" rmdir /S /Q "src-tauri\target\release\bundle\nsis"
call npx tauri build || exit /b 1

echo.
echo  Collecting artefacts...
if not exist "dist-release" mkdir "dist-release"

REM A previous artefact that is still running cannot be replaced: Windows holds
REM a lock on a loaded image. Deleting first turns that into a visible failure
REM here rather than a stale file that silently ships. Note that an elevated
REM instance cannot be stopped from a normal shell, so close the app yourself
REM before building.
del /Q "dist-release\*.exe" >nul 2>nul
if exist "dist-release\*.exe" (
  echo  [!] dist-release still holds an .exe that could not be deleted.
  echo      AllInsight is probably still running. Close it and build again.
  exit /b 1
)

REM Check each copy's own exit status. Testing only that the file exists would
REM accept a stale artefact left behind by a failed overwrite.
copy /Y "src-tauri\target\release\allinsight.exe" "dist-release\AllInsight.exe" >nul 2>nul
if errorlevel 1 (
  echo  [!] The executable could not be copied into dist-release.
  exit /b 1
)
copy /Y "src-tauri\target\release\bundle\nsis\*.exe" "dist-release\" >nul 2>nul
if errorlevel 1 (
  echo  [!] The installer could not be copied into dist-release.
  exit /b 1
)

echo.
echo  Done.
echo.
echo    Executable : src-tauri\target\release\allinsight.exe
echo    Installer  : src-tauri\target\release\bundle\nsis\
echo    Copies     : dist-release\
echo.
echo  The installer is unsigned, so SmartScreen will warn on first run.
echo  Choose "More info" then "Run anyway".
echo.

endlocal
