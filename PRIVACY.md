# Privacy

**Short version: the AllInsight app collects nothing.** No account, no telemetry, no crash reporting, no analytics, no advertising identifiers. AllInsight makes no network connections unless you ask it to check for updates, or turn on automatic update checks.

_Last updated: October 2026._

## What the app stores on your device

All of this stays in your user profile. Uninstalling with "remove data" deletes it.

| Data | Why | Details |
|---|---|---|
| File metadata cache | Faster scans | Paths, sizes and dates. **File contents are never read for indexing.** |
| File hashes | Duplicate detection | Computed only for files with matching sizes; never leave your device. |
| Settings | Your preferences | Thresholds, protected folders, appearance. |
| Cleanup history | So you can see what was cleaned | Date, category and total size. **Not individual file names.** |
| Logs | Troubleshooting | Never include file contents, passwords or tokens. Personal file names are avoided. |

## The AI assistant

The optional AI model runs entirely on your computer. It reads a summary prepared by AllInsight (sizes, counts, drive readings) and never sends prompts or files anywhere. Models are only installed when you import one yourself.

## Diagnostics

"Export diagnostics" creates a file on your device. Nothing is sent. Review it before you share it with anyone.

## Network access

The app has no telemetry or cloud features. Its one online feature is **checking for updates** (Settings → Updates):

- Off by default. AllInsight contacts the update server only when you click *Check for Updates*, or once a day if you choose *Automatically check for updates*.
- A check downloads one public file, the same for everyone. The request carries no account, no installation ID, no cookies, and nothing about your device, files or usage.
- AllInsight's update mechanism does not upload user analytics, documents, datasets, reports, AI interactions, or personal data.
- Downloading and installing an update only happens when you click *Update Now* and then *Restart Now*.

The full details are in [docs/PRIVACY.md](docs/PRIVACY.md) and [docs/UPDATE_SYSTEM.md](docs/UPDATE_SYSTEM.md).

## Contact

Open an issue on [GitHub](https://github.com/Sk1750-alt/AllInsight) or reach the maintainer directly.
