<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/allinsight-lockup-dark.svg">
    <img src=".github/assets/allinsight-lockup-light.svg" alt="AllInsight" width="360">
  </picture>
</p>

<p align="center"><b>Your device, understood.</b><br>
Local device intelligence for Windows. Open source, offline, and free of telemetry.</p>

<p align="center">
  <a href="LICENSE"><img alt="License: GPL-3.0-or-later" src="https://img.shields.io/badge/license-GPL--3.0--or--later-blue"></a>
  <a href="https://github.com/OWNER/allinsight/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/OWNER/allinsight/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://scorecard.dev/viewer/?uri=github.com/OWNER/allinsight"><img alt="OpenSSF Scorecard" src="https://api.scorecard.dev/projects/github.com/OWNER/allinsight/badge"></a>
  <a href="https://api.reuse.software/info/github.com/OWNER/allinsight"><img alt="REUSE status" src="https://api.reuse.software/badge/github.com/OWNER/allinsight"></a>
  <!-- After you register at https://www.bestpractices.dev, replace PROJECT_ID and uncomment:
  <a href="https://www.bestpractices.dev/projects/PROJECT_ID"><img alt="OpenSSF Best Practices" src="https://www.bestpractices.dev/projects/PROJECT_ID/badge"></a>
  -->
</p>

---

> **Status: pre-release.** AllInsight is under active development and not yet ready for everyday use. Watch the repository or join the early-access list at https://YOUR-DOMAIN to hear when the first release is out.

## What it is

AllInsight shows where your storage went, how your drive is holding up, and what is safe to clean. An AI model that runs entirely on your laptop explains everything in plain language.

- **Storage map:** see which folders take the space and drill into any of them.
- **Safe cleanup:** temporary files and caches only. Personal files are never removed automatically.
- **Large files and duplicates:** duplicates are matched by size and content hash, not by name.
- **Drive health:** SMART and NVMe readings, reported only when your drive provides them.
- **Performance:** processor, memory, graphics, disk, network and battery, with the process behind each spike.
- **Startup apps and installed apps:** switch off startup items; uninstall through Windows' own mechanism.
- **Local AI (optional):** a GGUF model running through llama.cpp reads a summary AllInsight prepares and explains it. It cannot delete files or run commands.

## Principles

1. **Everything stays on this device.** No account, no cloud, no telemetry. AllInsight makes no network connections unless you explicitly ask it to.
2. **Never guess with your files.** Every item is sorted into Safe, Review or Protected before anything can happen. Only Safe items can be cleaned automatically. See [CLEANUP_RULES.md](CLEANUP_RULES.md).
3. **No scare tactics.** No registry cleaning, no "speed boost" claims, no inflated error counts.
4. **Never fabricate data.** If a drive doesn't report a value, AllInsight says "Not reported".
5. **The AI explains; you decide.** All actions run through a typed, allowlisted Rust safety layer, and only after you approve them.

## Install

| Channel | Status |
|---|---|
| Microsoft Store | Coming soon |
| winget (`winget install AllInsight.AllInsight`) | Coming soon |
| [GitHub Releases](https://github.com/OWNER/allinsight/releases) | Coming soon |

Requirements: Windows 10 or 11, 64-bit (x64). Internet is not required.

### Verify your download

Every release includes `SHA256SUMS.txt`, a software bill of materials (SBOM), and a signed build-provenance attestation produced by GitHub Actions.

```powershell
# 1. Check the checksum
Get-FileHash .\AllInsight_x.y.z_x64-setup.exe -Algorithm SHA256

# 2. Check the file was built from this repository by our release workflow
gh attestation verify .\AllInsight_x.y.z_x64-setup.exe --repo OWNER/allinsight
```

See [CODE_SIGNING_POLICY.md](CODE_SIGNING_POLICY.md) for how releases are signed.

## Build from source

Prerequisites: [Rust](https://rustup.rs) (stable), [Node.js](https://nodejs.org) 20 or later, Microsoft Visual Studio Build Tools with the "Desktop development with C++" workload, and the WebView2 runtime (preinstalled on Windows 11).

```powershell
git clone https://github.com/OWNER/allinsight.git
cd allinsight
npm ci
npm run tauri dev      # run in development
npm run tauri build    # produce installers in src-tauri/target/release/bundle
```

## Project layout

```
src/                 React + TypeScript interface
src-tauri/           Rust backend: scanning, monitoring, safety layer, AI process management
rules/               Machine-readable cleanup rules (see CLEANUP_RULES.md)
website/             Static files for the project website
```

## Contributing

Contributions are welcome, including translations (Bengali and Hindi especially). Read [CONTRIBUTING.md](CONTRIBUTING.md) first. Every commit needs a DCO sign-off (`git commit -s`).

Changes to cleanup rules get extra review. A rule that removes the wrong file is the worst bug this project can have.

## Security

Please report vulnerabilities privately. See [SECURITY.md](SECURITY.md). Any way to make AllInsight delete a Protected or Review item without explicit approval is treated as critical.

## Privacy

AllInsight collects nothing. Details in [PRIVACY.md](PRIVACY.md).

## License

AllInsight is free software: you can redistribute it and/or modify it under the terms of the [GNU General Public License](LICENSE), version 3 or (at your option) any later version.

The AllInsight name and logo are not covered by the GPL. See [TRADEMARKS.md](TRADEMARKS.md).
