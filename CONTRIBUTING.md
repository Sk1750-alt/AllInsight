# Contributing to AllInsight

Thank you for helping. This guide covers how to set up, what we expect in a pull request, and the extra rules for code that can delete files.

## Ground rules

- Be kind. Read the [Code of Conduct](CODE_OF_CONDUCT.md).
- Security issues go through [SECURITY.md](SECURITY.md), never public issues.
- Open an issue before starting large changes, so we can agree on the approach first.

## Development setup

You need [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 20+.

**Windows** additionally requires Visual Studio Build Tools 2022 with "Desktop development with C++", the Windows 10/11 SDK, and the WebView2 runtime (preinstalled on Windows 11).

**Linux** additionally requires the Tauri build dependencies:

```sh
# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential file rpm xdg-utils
# Fedora
sudo dnf install webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel openssl-devel rpm-build
# Arch
sudo pacman -S --needed webkit2gtk-4.1 libayatana-appindicator librsvg xdotool base-devel
```

Then:

```sh
npm ci
npm run tauri:dev
```

Before you push:

```sh
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cd ..
npm run typecheck
npm run build
```

## Developer Certificate of Origin (DCO)

Every commit must be signed off. This certifies that you wrote the change or have the right to submit it under this project's license, as described in the [Developer Certificate of Origin 1.1](https://developercertificate.org).

```sh
git commit -s -m "Fix treemap drill-down on network drives"
```

This adds a line like `Signed-off-by: Your Name <you@example.com>`. Use your real name. Pull requests with unsigned commits can't be merged. To sign off commits you already made: `git rebase --signoff main`.

We use a DCO instead of a Contributor License Agreement. You keep the copyright on your contribution, and the project stays MIT-licensed.

## Pull requests

- Keep each PR focused on one change.
- Add or update tests for behaviour changes.
- Update documentation when behaviour changes.
- Describe what you tested and on which OS/version.
- CI must pass.

## Changes to cleanup rules or the safety layer

Anything that touches the cleanup, security or safety modules gets stricter review:

1. **Explain why the location is safe.** Link to vendor or OS documentation showing the data is regenerated automatically.
2. **Never widen the Safe tier casually.** New Safe rules need the narrowest possible path, a minimum file age, and a running-process check where an app holds the data.
3. **Add tests** proving the rule does not match anything under Protected locations, including through symbolic links, junctions and OneDrive folders.
4. **Update documentation** in the same PR so the docs and the rules never drift apart.
5. Maintainer approval is required, and these PRs are never merged on the same day they are opened.

## Coding style

- **Rust:** `rustfmt` defaults and `clippy` with warnings denied. No `unsafe` outside the platform API wrapper modules, and every `unsafe` block needs a `// SAFETY:` comment.
- **TypeScript:** the repository's TypeScript strict mode configuration.
- **Interface text:** short, plain, sentence case. No exclamation marks, no fear-based wording. Say "Storage usage is high", not "Your PC is in danger!".

## Translations

Translations are welcome. Translate meaning, not word-for-word, and keep the calm, plain tone.

## Licensing of contributions

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
