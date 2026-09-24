#!/usr/bin/env bash
# ===================================================================
#  AllInsight - production Linux build
#
#  Runs the type check, the Rust test suite, the frontend build and
#  the Tauri bundler, then copies the .deb, .rpm and .AppImage into
#  dist-release/. Stops at the first failure rather than shipping
#  something broken. The Linux counterpart of BUILD_WINDOWS.bat.
#
#  Build dependencies (Debian/Ubuntu names):
#    libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
#    libxdo-dev libssl-dev build-essential file rpm xdg-utils
#  Arch:   webkit2gtk-4.1 libayatana-appindicator librsvg xdotool base-devel
#  Fedora: webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel
#          libxdo-devel openssl-devel rpm-build
#
#  Build release packages on the oldest distribution you support: an
#  AppImage only runs on systems whose glibc is at least as new as the
#  one it was built against.
# ===================================================================

set -euo pipefail
cd "$(dirname "$0")/.."

echo
echo " AllInsight - production build (Linux)"
echo " ======================================"
echo

[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

command -v cargo >/dev/null || { echo " [!] Rust was not found. Install it from https://rustup.rs"; exit 1; }
command -v node  >/dev/null || { echo " [!] Node.js was not found. Install Node.js 20 or later."; exit 1; }
if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
  echo " [!] webkit2gtk-4.1 development files were not found. See the list at the top of this script."
  exit 1
fi

if [ ! -d node_modules ]; then
  echo " [1/5] Installing dependencies..."
  npm ci
else
  echo " [1/5] Dependencies already installed."
fi

echo
echo " [2/5] Type-checking the interface..."
npm run typecheck

echo
echo " [3/5] Running the Rust test suite..."
(cd src-tauri && cargo test)

echo
echo " [4/5] Building the frontend..."
npm run build

echo
echo " [5/5] Building the application and packages..."
rm -rf src-tauri/target/release/bundle
npx tauri build

mkdir -p dist-release
cp src-tauri/target/release/bundle/deb/*.deb \
   src-tauri/target/release/bundle/rpm/*.rpm \
   src-tauri/target/release/bundle/appimage/*.AppImage \
   dist-release/

echo
echo " Done. Packages are in dist-release/:"
(cd dist-release && sha256sum -- *.deb *.rpm *.AppImage)
