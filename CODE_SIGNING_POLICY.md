# Code Signing Policy

Release builds of AllInsight are signed so Windows can verify they originate from this project and have not been modified or tampered with.

Free code signing is provided by [SignPath.io](https://about.signpath.io), with certificates issued by the [SignPath Foundation](https://signpath.org).

> **Status:** the application to SignPath Foundation is pending. Until it is approved, release installers are **not** Authenticode-signed and Windows SmartScreen will warn on first run. Every release is still published with SHA-256 checksums and a build-provenance attestation, and in-app updates are verified against a separate Ed25519 update key (see [docs/UPDATE_SYSTEM.md](docs/UPDATE_SYSTEM.md)).

## What Gets Signed

Only artifacts built by the `release` job of the [build workflow](.github/workflows/build.yml) on GitHub Actions from a tagged commit (`v*.*.*`) in this repository. Nothing built on a personal machine is signed.

Each release also publishes:
- `SHA256SUMS.txt` with SHA-256 checksums of every release file.
- SBOMs (CycloneDX) for Rust and npm dependencies.
- A GitHub build-provenance attestation, verifiable with:
  ```bash
  gh attestation verify <installer.exe> --repo Sk1750-alt/AllInsight
  ```

## Team Roles & Security

| Role | Responsibility |
|---|---|
| Maintainers | Write code, review pull requests, create release tags |
| Approvers | Approve signing requests on SignPath |

All team members must maintain Multi-Factor Authentication (MFA / 2FA) on GitHub and on SignPath.

## Third-Party Dependencies

Third-party dependencies are compiled from source or verified packages during CI builds. Third-party binaries are never signed under our key.

## Reporting Misuse

If you discover a signed binary claiming to be AllInsight that was not produced from an official release in this repository, report it immediately via [SECURITY.md](SECURITY.md).
