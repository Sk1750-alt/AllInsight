# Code signing policy

Release builds of AllInsight are signed so Windows can verify they came from this project and were not modified.

<!-- Activate this line only after SignPath Foundation accepts the project:
Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).
-->

**Current status:** pre-release. Builds are not yet signed with a code-signing certificate. Microsoft Store builds are signed by Microsoft.

## What gets signed

Only artifacts built by the [release workflow](.github/workflows/release.yml) on GitHub Actions from a tagged commit in this repository. Nothing built on a personal machine is ever signed.

Each release also publishes:
- `SHA256SUMS.txt` with checksums of every file
- SBOMs (CycloneDX) for Rust and npm dependencies
- A GitHub build-provenance attestation, verifiable with `gh attestation verify <file> --repo OWNER/allinsight`

## Team roles

| Role | People | Responsibility |
|---|---|---|
| Committers and reviewers | [@OWNER](https://github.com/OWNER) | Write and review code |
| Approvers | [@OWNER](https://github.com/OWNER) | Approve each signing request |

All team members use multi-factor authentication on GitHub and on the signing service.

## Upstream dependencies

Third-party libraries are included as source dependencies and built in CI. We do not sign third-party binaries. The AI model is never bundled; users import GGUF models themselves.

## Privacy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it. See [PRIVACY.md](PRIVACY.md).

## Reporting a signed malicious build

If you find a signed file claiming to be AllInsight that wasn't built from this repository, report it immediately via [SECURITY.md](SECURITY.md).
