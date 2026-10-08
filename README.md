<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/allinsight-lockup-dark.svg">
    <img src=".github/assets/allinsight-lockup-light.svg" alt="AllInsight" width="360">
  </picture>
</p>

<p align="center"><b>Your device, understood.</b><br>
Private by design. Intelligent by default.</p>

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue?style=flat-square"></a>
  <a href="https://github.com/Sk1750-alt/AllInsight/actions/workflows/build.yml"><img alt="CI" src="https://github.com/Sk1750-alt/AllInsight/actions/workflows/build.yml/badge.svg"></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-lightgrey?style=flat-square">
  <img alt="Version" src="https://img.shields.io/badge/version-1.0.0-green?style=flat-square">
  <img alt="Made with Tauri" src="https://img.shields.io/badge/built%20with-Tauri%20v2-FFC131?style=flat-square&logo=tauri&logoColor=white">
</p>

---

AllInsight is an open-source desktop application for **Windows** and **Linux** that analyses storage, monitors device health, finds what is genuinely safe to clean, and explains what it finds — all on your own machine.

No account. No telemetry. Every feature works with the network disconnected. The only time AllInsight goes online is to check for updates, when you ask it to.

## ✨ Features

| Screen | What it gives you |
| --- | --- |
| **Overview** | A device health score with its reasons, four vital signs, ranked recommendations, and a plain-language summary. |
| **Storage Map** | A squarified treemap of one folder level at a time, plus a category breakdown of everything scanned. |
| **Large Files** | The biggest files on the device, with a risk classification and a route to the Recycle Bin. |
| **Duplicates** | Files with identical contents, found by size grouping → partial hash → full BLAKE3. Never by name. |
| **Cleanup** | Thirteen typed cleanup categories, a dry run, and a confirmation that states what will and will not be touched. |
| **Performance** | Live CPU, memory, GPU, disk and network with ten minutes of history. |
| **Processes** | A process list with publisher, resource use, and an End button that refuses on critical OS processes. |
| **Startup** | Startup entries from the Run keys and Startup folders, toggled the same way the OS does it. |
| **Applications** | Installed applications with measured sizes, uninstalled through the vendor's own uninstaller. |
| **Battery** | Charge, design capacity, cycle count and health — shown only where the hardware reports them. |
| **Drive Health** | Model, bus, wear, temperature, power-on hours and error counts, with `Unknown` where the drive is silent. |
| **AllInsight AI** | A local assistant (optional) that explains measurements — it cannot act on anything. |
| **Activity** | Cleanup history and an event log, recorded as totals rather than file lists. |
| **Settings** | Eleven sections, including the real protected-path list. Every switch does something. |

## 🔒 Principles

1. **Everything stays on this device.** No account, no cloud, no telemetry. The only network request AllInsight can make is an update check you start or allow, and it carries nothing about you ([how updates work](docs/UPDATE_SYSTEM.md)).
2. **Never guess with your files.** Every item is sorted into Safe, Review or Protected before anything can happen. Only Safe items can be cleaned automatically.
3. **No scare tactics.** No registry cleaning, no "speed boost" claims, no inflated error counts.
4. **Never fabricate data.** If a drive doesn't report a value, AllInsight says "Not reported" — never a guess.
5. **The AI explains; you decide.** All actions run through a typed, allowlisted Rust safety layer, and only after you approve them.

## 📥 Install

| Channel | Status |
|---|---|
| [GitHub Releases](https://github.com/Sk1750-alt/AllInsight/releases) | `v1.0.0` |
| AUR (`allinsight-bin`) | [PKGBUILD](packaging/arch/PKGBUILD) |

**Windows:** Windows 10 (1809+) or 11, x64. The installer adds the Edge WebView2 runtime if missing.
**Linux:** Any x64 glibc distribution with WebKitGTK 4.1. `.deb`, `.rpm` and AppImage provided.

## 🛠 Build from source

**Prerequisites:** [Rust](https://rustup.rs) stable, [Node.js](https://nodejs.org) 20+, and platform-specific dependencies.

<details>
<summary><strong>Windows</strong></summary>

- Visual Studio Build Tools 2022 with **Desktop development with C++**
- Windows 10/11 SDK
- WebView2 runtime (preinstalled on Windows 11)

```bat
npm install
npm run tauri:dev        :: development with hot reload
BUILD_WINDOWS.bat        :: production build → dist-release\
```
</details>

<details>
<summary><strong>Linux</strong></summary>

```sh
# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential file rpm xdg-utils

# Fedora
sudo dnf install webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel openssl-devel rpm-build

# Arch
sudo pacman -S --needed webkit2gtk-4.1 libayatana-appindicator librsvg xdotool base-devel
```

```sh
npm install
npm run tauri:dev            # development with hot reload
bash scripts/build-linux.sh  # production build → dist-release/
```
</details>

### Other useful commands

```sh
npm run typecheck     # TypeScript type-check, no emit
npm test              # Rust test suite, including the safety tests
npm run icons         # regenerate icon set from the SVG source
```

## 🏗 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  WebView2 / WebKitGTK window                                │
│  React + TypeScript. Renders state and collects intent.     │
│  Holds no privilege and makes no decision that matters.     │
└───────────────────────────┬─────────────────────────────────┘
                            │  Tauri IPC, typed both sides
┌───────────────────────────▼─────────────────────────────────┐
│  commands/   — validates arguments, calls one service       │
└───────────────────────────┬─────────────────────────────────┘
┌───────────────────────────▼─────────────────────────────────┐
│  services/                                                  │
│  security  storage  cleanup  system  process                │
│  health    battery  apps     startup  ai       db           │
└───────────────────────────┬─────────────────────────────────┘
                            │  Win32 / WMI / UDisks2 / sysfs
┌───────────────────────────▼─────────────────────────────────┐
│  Operating System                                           │
└─────────────────────────────────────────────────────────────┘

           ┌──────────────────────────────────┐
           │  llama-server (child process)    │
           │  127.0.0.1 only. Optional.       │
           └──────────────────────────────────┘
```

### Project layout

```
src/                    React + TypeScript interface
  app/                  store, navigation
  components/           shell, logo, UI primitives
  views/                one file per screen
  lib/                  typed IPC client, formatting, utilities
src-tauri/              Rust backend
  src/commands/         the IPC surface, thin by design
  src/services/         all the actual work
    security/           protected paths, deletion guard  ← the safety model
    storage/            volumes, scanner, large files, duplicates
    cleanup/            categories, engine, removal, recycle bin
    system/ process/    live metrics and the process list
    health/ battery/    drive and battery reporting
    apps/ startup/      installed applications, startup entries
    ai/                 facts, deterministic insights, local model
    db/                 SQLite and settings
assets/logo/            SVG source of truth for the mark and icons
scripts/                icon generation, licence collection, build scripts
packaging/arch/         AUR PKGBUILD
docs/                   architecture, security model, AI documentation
```

> **Full details:** [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) · [`docs/SECURITY.md`](docs/SECURITY.md) · [`docs/AI-MODELS.md`](docs/AI-MODELS.md)

## 🛡 Safety model

The frontend **never** sends a file path to a destructive command. It sends a cleanup category variant or an opaque candidate ID the backend issued. Removal requires a `ValidatedPath` — a token that only the deletion guard can construct — and the guard re-runs every check at the moment of deletion:

1. Lexical normalisation (`.` / `..` resolution)
2. Protected-path engine (built from OS known folders, not hardcoded strings)
3. Allow-list membership per cleanup category
4. Existence check via `symlink_metadata`
5. Reparse point / symlink / junction rejection on entry and every parent
6. Full re-verification on the canonicalised path
7. Entry-kind consistency check

> **Full details:** [`docs/SECURITY.md`](docs/SECURITY.md)

## 🤖 Local AI (optional)

AllInsight works fully without a model. Every insight, recommendation and score comes from a deterministic generator. A model only adds conversation.

1. Install [llama.cpp](https://github.com/ggml-org/llama.cpp/releases)'s `llama-server`
2. Import any instruction-tuned GGUF model
3. Press **Load** on the AllInsight AI screen

The model runs as a child process on `127.0.0.1` with a per-launch API key. It explains measurements and cannot act on anything.

> **Full details:** [`docs/AI-MODELS.md`](docs/AI-MODELS.md)

## 🧪 Tests

```sh
npm test
```

**191 tests** covering path normalisation, protected-path classification, junction/symlink refusal, cleanup category invariants, guard rejections, duplicate detection, storage roll-up arithmetic, drive-health decision rules, settings clamping, and the database layer.

## 🤝 Contributing

Contributions are welcome, including translations. Read [CONTRIBUTING.md](CONTRIBUTING.md) first. Every commit needs a DCO sign-off (`git commit -s`).

Changes to the cleanup or safety layer get extra review. A rule that removes the wrong file is the worst bug this project can have.

## 🔐 Security

Please report vulnerabilities **privately**. See [SECURITY.md](SECURITY.md). Any way to make AllInsight delete a Protected or Review item without explicit approval is treated as critical.

## 🕵️ Privacy

AllInsight collects nothing. Details in [PRIVACY.md](PRIVACY.md).

## 📝 License

[MIT License](LICENSE) — free to use, modify, and distribute.

---

<p align="center">
  <sub>Built with <a href="https://v2.tauri.app">Tauri</a>, <a href="https://react.dev">React</a>, and <a href="https://www.rust-lang.org">Rust</a>.</sub>
</p>
