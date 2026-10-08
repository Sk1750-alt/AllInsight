# Privacy

**Short version: the AllInsight app collects nothing.** No account, no telemetry, no crash reporting, no analytics, no advertising identifiers. AllInsight makes no network connections unless you ask it to.

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

The app has no telemetry or cloud features. If an optional online feature is ever added (for example, checking for updates), it will be off by default, clearly labelled, and documented here before release.

## Contact

Open an issue on [GitHub](https://github.com/Sk1750-alt/AllInsight) or reach the maintainer directly.
