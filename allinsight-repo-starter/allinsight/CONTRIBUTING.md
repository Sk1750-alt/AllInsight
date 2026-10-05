# Contributing to AllInsight

Thank you for helping. This guide covers how to set up, what we expect in a pull request, and the extra rules for code that can delete files.

## Ground rules

- Be kind. Read the [Code of Conduct](CODE_OF_CONDUCT.md).
- Security issues go through [SECURITY.md](SECURITY.md), never public issues.
- Open an issue before starting large changes, so we can agree on the approach first.

## Development setup

You need Rust (stable), Node.js 20+, Visual Studio Build Tools with "Desktop development with C++", and the WebView2 runtime.

```powershell
npm ci
npm run tauri dev
```

Before you push:

```powershell
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cd ..
npm run lint
npm run build
```

## Developer Certificate of Origin (DCO)

Every commit must be signed off. This certifies that you wrote the change or have the right to submit it under this project's license, as described in the [Developer Certificate of Origin 1.1](https://developercertificate.org).

```powershell
git commit -s -m "Fix treemap drill-down on network drives"
```

This adds a line like `Signed-off-by: Your Name <you@example.com>`. Use your real name. Pull requests with unsigned commits can't be merged. To sign off commits you already made: `git rebase --signoff main`.

We use a DCO instead of a Contributor License Agreement. You keep the copyright on your contribution, and the project stays GPL-licensed for good.

## Pull requests

- Keep each PR focused on one change.
- Add or update tests for behaviour changes.
- Update documentation when behaviour changes.
- Describe what you tested and on which Windows version.
- CI must pass.

## Changes to cleanup rules or the safety layer

Anything that touches `rules/`, `CLEANUP_RULES.md`, or the Rust cleanup and safety modules gets stricter review:

1. **Explain why the location is safe.** Link to vendor or Microsoft documentation showing the data is regenerated automatically.
2. **Never widen the Safe tier casually.** New Safe rules need the narrowest possible path, a minimum file age, and a running-process check where an app holds the data.
3. **Add tests** proving the rule does not match anything under Protected locations, including through symbolic links, junctions and OneDrive folders.
4. **Update CLEANUP_RULES.md** in the same PR so the documentation and the rules never drift apart.
5. Maintainer approval is required, and these PRs are never merged on the same day they are opened.

## Coding style

- Rust: `rustfmt` defaults and `clippy` with warnings denied. No `unsafe` outside the Windows API wrapper module, and every `unsafe` block needs a `// SAFETY:` comment.
- TypeScript: the repository's ESLint and Prettier configuration.
- Interface text: short, plain, sentence case. No exclamation marks, no fear-based wording. Say "Storage usage is high", not "Your PC is in danger!".

## Translations

Bengali, Hindi and other translations are very welcome. Translate meaning, not word-for-word, and keep the calm, plain tone.

## Licensing of contributions

By contributing, you agree that your contributions are licensed under GPL-3.0-or-later. Please don't submit code copied from projects with incompatible licenses.
