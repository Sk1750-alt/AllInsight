# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org).

## [1.0.0] — 2026-10-08

### Added
- **Overview** — Device health score with vital signs, ranked recommendations and plain-language summary.
- **Storage Map** — Squarified treemap with per-folder drill-down and category breakdown.
- **Large Files** — Ranked by size with risk classification and Recycle Bin route.
- **Duplicates** — BLAKE3 content-based duplicate detection (size → partial hash → full hash).
- **Cleanup** — Thirteen typed cleanup categories with dry run and confirmation dialog.
- **Performance** — Live CPU, memory, GPU, disk and network with ten minutes of history.
- **Processes** — Process list with publisher, resource use, and safe-guarded end button.
- **Startup** — Startup entry management from Run keys and Startup folders.
- **Applications** — Installed applications with measured sizes and vendor uninstaller.
- **Battery** — Charge, design capacity, cycle count and health where hardware reports them.
- **Drive Health** — SMART and NVMe readings via WMI (Windows) and UDisks2 (Linux).
- **AllInsight AI** — Optional local assistant using llama.cpp over loopback, with API key authentication.
- **Activity** — Cleanup history and event log recorded as totals.
- **Settings** — Eleven configuration sections including the real protected-path list.
- Cross-platform support: Windows 10/11 (x64), Linux x64 (Ubuntu 22.04+, Debian 12+, Fedora 39+, Arch, and more).
- Experimental macOS support (CI-only).
- Full offline operation — no network requests, no telemetry, no accounts.
- Three-tier safety model (Safe → Review → Protected) with `ValidatedPath` tokens.
- Background monitor with volume capacity alerts and optional auto-clean.
- Single-instance enforcement via `tauri-plugin-single-instance`.
- Per-user NSIS installer (Windows), `.deb`, `.rpm` and AppImage (Linux).
- CI/CD pipeline for Windows, Linux (Ubuntu, Arch, Fedora) and macOS.

### Security
- `export_diagnostics` hardened against arbitrary file overwrite.
- AI engine executable restricted to `llama-server` / `server` binaries only.
- Loopback inference engine secured with per-launch API key.
- `split_command` parser fixed for unquoted paths with `.exe` in directory names.
- Startup toggle validated against real startup items before registry write.
- Model removal now checks for links in the folder chain, not just the target.
- Protected-path set changed to `Arc` snapshot to eliminate reader-writer lock contention.
