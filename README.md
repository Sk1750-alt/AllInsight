# AllInsight

**Your device, understood.**
Private by design. Intelligent by default.

AllInsight is a desktop application for Windows and Linux that analyses storage,
monitors device health, finds what is genuinely safe to clean, and explains what
it finds. All of it happens on the machine it is installed on. There is no account, no
server, and no telemetry, and every feature works with the network
disconnected.

---

## What it does

| Screen | What it gives you |
| --- | --- |
| **Overview** | A device health score with its reasons, four vital signs, ranked recommendations, and a plain-language summary. |
| **Storage Map** | A squarified treemap of one folder level at a time, plus a category breakdown of everything scanned. |
| **Large Files** | The biggest files on the device, with a risk classification and a route to the Recycle Bin. |
| **Duplicates** | Files with identical contents, found by size grouping then partial hash then full BLAKE3. Never by name. |
| **Cleanup** | Thirteen typed cleanup categories, a dry run, and a confirmation that states what will and will not be touched. |
| **Performance** | Live CPU, memory, GPU, disk and network with ten minutes of history. |
| **Processes** | A process list with publisher, resource use, and an End button that refuses on the processes Windows needs. |
| **Startup** | Startup entries from the Run keys and the Startup folders, enabled and disabled the same way Task Manager does it. |
| **Applications** | Installed applications with measured sizes, uninstalled through the vendor's own uninstaller. |
| **Battery** | Charge, design capacity, cycle count and health, shown only where the hardware actually reports them. |
| **Drive Health** | Model, bus, wear, temperature, power-on hours and error counts, with `Unknown` where the drive is silent. |
| **AllInsight AI** | A local assistant, optional, that explains measurements and cannot act on anything. |
| **Activity** | Cleanup history and an event log, recorded as totals rather than file lists. |
| **Settings** | Eleven sections, including the real protected-path list read back from the backend. Every switch on it does something; there are no decorative controls. |

---

## Platforms

| Platform | Status |
| --- | --- |
| Windows 10 (1809+) and 11, x64 | Fully supported. NSIS installer. |
| Linux, x64: Ubuntu 22.04+, Debian 12+, Fedora 39+, Arch, openSUSE, Mint, Pop!\_OS and other glibc distributions with WebKitGTK 4.1 | Fully supported. `.deb`, `.rpm` and AppImage. |
| macOS 11+ | Experimental: compiled and tested in CI only, not yet run by hand. Applications, Startup and Trash show "not available yet". |

What each screen reads on Linux:

| Screen | Source on Linux |
| --- | --- |
| Cleanup | Your own files only: the temporary folder, `~/.cache` thumbnails, fonts, Mesa/NVIDIA shader caches, browser caches (Chrome, Chromium, Edge, Brave, Vivaldi, Opera, Firefox including Snap), and the freedesktop Trash. Nothing outside your home or temporary folder, and never another user's file. |
| Applications | The desktop entries your launcher shows, traced to dpkg, rpm, pacman, Flatpak or Snap. Flatpak and Snap apps are removed directly; for apt, dnf, zypper and pacman packages AllInsight shows the exact command, because it never runs a package manager as root. |
| Startup | XDG autostart (`~/.config/autostart`, `/etc/xdg/autostart`). Disabling writes a per-user `Hidden=true` override, exactly as GNOME and KDE do. |
| Drive Health | UDisks2 over D-Bus (SMART failure prediction, temperature, power-on hours, bad sectors, NVMe critical warnings), falling back to sysfs. No root needed. |
| Battery | `/sys/class/power_supply`. |
| GPU | `/sys/class/drm`, `amdgpu` busy counters, and `nvidia-smi` when present. |
| Processes | Refuses to end init, the compositor, the display server, the login manager and the session bus; asks first for PipeWire, NetworkManager and the file manager. Ends processes with SIGTERM, not SIGKILL. |

Whole-disk scans skip `/proc`, `/sys`, `/dev`, `/run`, tmpfs, Snap squashfs
images, container overlays and network or WSL shares, so they measure disk and
nothing else. Everything outside `/home`, `/tmp`, `/mnt` and `/media` is on the
protected list.

## Requirements

**To run on Windows:** Windows 10 (1809 or later) or Windows 11, x64. The installer adds
the Microsoft Edge WebView2 runtime if it is missing. Windows 11 ships with it;
on a Windows 10 machine without it, the installer downloads it, so that one
step needs a connection. The application itself never does.

**To run on Linux:** WebKitGTK 4.1 (preinstalled on GNOME and KDE desktops;
the `.deb` and `.rpm` pull it in). Optional: `udisks2` for drive health,
`libayatana-appindicator3` for the tray icon, and on GNOME the AppIndicator
extension to see it.

**To build on Linux:** Rust stable, Node.js 20+, and the Tauri prerequisites:

```sh
# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential file rpm xdg-utils
# Fedora
sudo dnf install webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel openssl-devel rpm-build
# Arch
sudo pacman -S --needed webkit2gtk-4.1 libayatana-appindicator librsvg xdotool base-devel
```

Then `bash scripts/build-linux.sh`, which mirrors `BUILD_WINDOWS.bat` and puts
the `.deb`, `.rpm` and `.AppImage` in `dist-release/`. Build release packages
on the oldest distribution you support (CI uses Ubuntu 22.04): an AppImage only
runs where glibc is at least as new as the one it was built against. An AUR
`PKGBUILD` that repackages the `.deb` is in `packaging/arch/`.

**To build on Windows:**

- [Rust](https://rustup.rs) stable, `x86_64-pc-windows-msvc`
- Visual Studio Build Tools 2022 with **Desktop development with C++**
- The Windows 10/11 SDK
- Node.js 20 or later

---

## Running in development

```bat
npm install
npm run tauri:dev
```

The window opens on the Vite dev server with hot reload for the interface;
changing Rust triggers a rebuild.

Other useful commands:

```bat
npm run typecheck     :: TypeScript, no emit
npm test              :: the Rust test suite, including the safety tests
npm run icons         :: regenerate the Windows icon set from the SVG
```

---

## Building the installer

```bat
BUILD_WINDOWS.bat
```

That script runs the type check, the Rust tests, the production frontend
build, and `tauri build`, then copies the results into `dist-release/`.

Or step by step:

```bat
npm install
npm run build
npx tauri build
```

### Where the output lands

| Artefact | Path |
| --- | --- |
| Installer | `src-tauri/target/release/bundle/nsis/AllInsight_1.0.0_x64-setup.exe` |
| Executable | `src-tauri/target/release/allinsight.exe` |
| Copies of both | `dist-release/` |

That executable is self-contained apart from WebView2, which every current
Windows 11 machine already has. Copying it alone to another machine works as a
portable build; the installer is the supported route because it also registers
the uninstaller and the Start menu entry.

The installer is unsigned, so Windows SmartScreen will show a warning the first
time it runs. Choose **More info**, then **Run anyway**. Signing it requires a
code-signing certificate, which is a purchase rather than a build step.

---

## Running completely offline

Nothing needs to be done. AllInsight contains no code that makes an outbound
network request:

- there is no update checker, no analytics, and no cloud service
- the local model runs as a child process bound to `127.0.0.1`
- the only HTTP client in the binary has TLS compiled out, because loopback is
  the only address it is ever pointed at

Disconnect the machine and every screen behaves identically. AllInsight shows
"Offline mode - all local features available" rather than an error.

---

## Installing a local model

AllInsight works fully without one. Every insight, recommendation and score comes
from a deterministic generator that needs no model at all. A model only adds
conversation.

1. **Get the engine.** Download a llama.cpp release from
   <https://github.com/ggml-org/llama.cpp/releases> and put `llama-server.exe`
   (Windows) or `llama-server` (Linux, marked executable) in:

   ```
   %LOCALAPPDATA%\AllInsight\engine\        Windows
   ~/.local/share/AllInsight/engine/         Linux
   ```

   On Linux a `llama-server` on `PATH` (from the AUR, Nix or a source build)
   is found automatically.

2. **Get a model.** Any instruction-tuned GGUF works. Sizing:

   | Installed RAM | Suggested model | On disk |
   | --- | --- | --- |
   | 8 GB | a 3B model at Q4_K_M | about 2 GB |
   | 16 GB | a 7B or 8B model at Q4_K_M | about 4-5 GB |
   | 32 GB or more | a 7B or 8B model at Q4_K_M or Q5_K_M | about 5-6 GB |

3. **Import it.** Open **AllInsight AI**, choose **Import a GGUF model**, and pick
   the file. AllInsight validates the GGUF header before copying it into
   `%LOCALAPPDATA%\AllInsight\models\`.

4. **Load it.** Press **Load**. The engine starts as a separate process, so a
   model that misbehaves cannot take the interface with it.

AllInsight never downloads a model. There is no download code in the binary.

---

## Permissions

AllInsight runs as a standard user and never asks for elevation at launch. On
Linux it never runs as root at all: everything it cleans belongs to you, and
drive health comes from UDisks2 without privilege. On Windows three things need
administrator permission, and each says so where it matters:

| Feature | Why |
| --- | --- |
| Drive wear and error counts on SATA drives | `MSFT_StorageReliabilityCounter` is readable only by an elevated process on most machines. NVMe drives report wear, temperature and power-on hours from their own health log without it. |
| Windows Update, Delivery Optimization and servicing log cleanup | Those folders are owned by SYSTEM. |
| Toggling machine-wide startup entries | They live under `HKEY_LOCAL_MACHINE`. |

Everything else, including all user-scoped cleanup, the storage map, duplicate
detection, process management and per-user startup entries, works unelevated.
Without elevation, drive health reads `Unknown` with an explanation, never a
guess.

---

## Project layout

```
/src                    React + TypeScript interface
  /app                  store, navigation
  /components           shell, logo, UI primitives
  /views                one file per screen
  /lib                  typed IPC client, formatting, utilities
/src-tauri              Rust backend
  /src/commands         the IPC surface, thin by design
  /src/services         all the actual work
    /security           protected paths, deletion guard  <- the safety model
    /storage            volumes, scanner, large files, duplicates
    /cleanup            categories, engine, removal, recycle bin
    /system /process    live metrics and the process list
    /health /battery    drive and battery reporting
    /apps /startup      installed applications, startup entries
                        (windows.rs / linux.rs per platform)
    /ai                 facts, deterministic insights, local model
    /db                 SQLite and settings
/assets/logo            SVG source of truth for the mark and icons
/scripts                icon generation, licence collection, build-linux.sh
/packaging/arch         AUR PKGBUILD
/docs                   architecture, security model, AI notes
```

---

## Safety model in one paragraph

The frontend never sends a path to a destructive command. It sends a cleanup
category variant or an opaque candidate id the backend issued. Removal requires
a `ValidatedPath`, a token that only the deletion guard can construct, and the
guard re-runs every check at the moment of deletion: lexical normalisation, the
protected-path engine, membership of that category's compiled-in allow-list,
existence, reparse-point rejection on the entry and every directory above it,
and then all of it again on the fully canonicalised path. The full reasoning is
in [`docs/SECURITY.md`](docs/SECURITY.md), and the tests that hold it in place
are in the `security` and `cleanup` modules.

---

## Tests

```bat
npm test
```

The suite covers path normalisation and traversal, protected-path
classification, junction and symlink refusal, cleanup category invariants,
guard rejections, duplicate detection including the middle-of-file case that
partial hashing alone would miss, storage roll-up arithmetic, drive-health
decision rules, settings clamping, and the database layer.

There is also a timing probe, excluded from the normal run, that reports how
long cleanup discovery takes per category on the machine it runs on:

```bat
cd src-tauri
cargo test --test timing -- --nocapture --ignored
```
