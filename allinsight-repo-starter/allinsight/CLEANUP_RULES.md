# Cleanup rules

_Specification v0.1 (draft). Maintainers must review and test every rule on real Windows 10 and 11 machines before the first release._

This document is the public contract for what AllInsight may and may not remove. The machine-readable version is [`rules/cleanup-rules.toml`](rules/cleanup-rules.toml). The two must always match; CI should fail if they drift.

## The three tiers

| Tier | Meaning | Automatic cleanup | Where removed items go |
|---|---|---|---|
| **Safe** | Data that Windows or an app regenerates on its own | Allowed, only if the user turns on Auto-Clean | Deleted permanently (it rebuilds itself) |
| **Review** | Things only the user can judge | **Never** | Recycle Bin, after explicit selection |
| **Protected** | User work, system files, and anything unknown | **Never**, and never offered | Not touched |

**Anything that matches no rule is Protected.** Unknown means untouched.

## Global safety invariants

These apply to every operation, in this order, and are enforced in Rust, not in the interface or the AI:

1. **Typed operations only.** Cleanup runs through a fixed set of `CleanupOperation` enum variants. No operation accepts a free-form path, command or string from the interface or the AI model.
2. **Canonicalize first.** Every path is resolved to its final form before any check.
3. **Never follow reparse points.** Symbolic links, junctions, mount points and cloud placeholder files (OneDrive, Dropbox, Google Drive "files on demand") are skipped, never traversed and never deleted. Deleting a cloud placeholder can delete the file in the cloud.
4. **Protected wins.** A path is checked against the Protected list *after* it matches a Safe or Review rule. Any Protected match blocks the action.
5. **Stay inside the rule root.** After canonicalization, the path must still be inside the root of the rule that matched it.
6. **Skip what's in use.** Locked files and files belonging to running processes are skipped, not forced.
7. **Minimum age.** Temporary files younger than the rule's `min_age_hours` (default 24) are skipped, so installers and running apps aren't disrupted.
8. **What you saw is what gets removed.** The confirmation screen comes from a manifest. Execution uses that same manifest and re-checks every entry (existence, size, modified time, invariants 2–7) immediately before removal. Anything that changed is skipped.
9. **Elevation only when needed.** Rules marked `requires_admin` are offered only when the user chooses to elevate, with a reason shown first.
10. **Aggregate logging.** Cleanup history records category and total size, not individual personal file names.

## Safe tier

| Rule ID | Location | Conditions |
|---|---|---|
| `user_temp` | `%TEMP%` (normally `%LOCALAPPDATA%\Temp`) | Older than 24 h, not locked |
| `windows_temp` | `%SystemRoot%\Temp` | Older than 24 h, not locked, requires admin |
| `crash_dumps_user` | `%LOCALAPPDATA%\CrashDumps\*.dmp` | Not locked |
| `crash_dumps_system` | `%SystemRoot%\Minidump\*.dmp`, `%SystemRoot%\MEMORY.DMP` | Requires admin |
| `error_reports` | `%LOCALAPPDATA%\Microsoft\Windows\WER`, `%ProgramData%\Microsoft\Windows\WER\ReportArchive`, `...\ReportQueue` | ProgramData paths require admin |
| `thumbnail_cache` | `%LOCALAPPDATA%\Microsoft\Windows\Explorer\thumbcache_*.db` | Skipped while Explorer holds them |
| `directx_shader_cache` | `%LOCALAPPDATA%\D3DSCache` | Not locked |
| `nvidia_shader_cache` | `%LOCALAPPDATA%\NVIDIA\DXCache`, `%LOCALAPPDATA%\NVIDIA\GLCache` | Not locked |
| `amd_shader_cache` | `%LOCALAPPDATA%\AMD\DxCache`, `%LOCALAPPDATA%\AMD\GLCache` | Not locked |
| `chrome_cache` | `...\Google\Chrome\User Data\<profile>\Cache` and `Code Cache` only | Chrome not running |
| `edge_cache` | `...\Microsoft\Edge\User Data\<profile>\Cache` and `Code Cache` only | Edge not running |
| `firefox_cache` | `%LOCALAPPDATA%\Mozilla\Firefox\Profiles\<profile>\cache2` | Firefox not running |
| `windows_update_cache` | `%SystemRoot%\SoftwareDistribution\Download` | Windows Update not active, requires admin |
| `delivery_optimization` | Via the Windows Delivery Optimization API, not direct deletion | Requires admin |
| `recycle_bin` | Emptied via the Windows shell API | **Always asks first. Never part of Auto-Clean.** |

Browser rules touch only cache folders. Cookies, history, saved passwords, bookmarks and extensions are Protected.

## Review tier

Shown to the user with name, location, size and date. Nothing here is ever selected by default or removed automatically.

| Rule ID | What it finds |
|---|---|
| `old_downloads` | Files in the Downloads folder not modified for 90 days (configurable) |
| `large_files` | Files above the size the user chooses (100 MB, 500 MB, 1 GB, 5 GB, or custom) |
| `duplicates` | Groups of files with identical size and identical full SHA-256 hash. The user picks which copy to keep. |
| `old_installers` | `.exe` and `.msi` files in Downloads older than 30 days |
| `old_archives` | `.zip`, `.rar`, `.7z` files older than 90 days |
| `disk_images` | `.iso` and `.img` files |
| `windows_old` | `C:\Windows.old`, removed only through Windows' own cleanup mechanism |

Duplicate detection never relies on file names. Files are grouped by size, then by a partial hash, then confirmed by a full hash.

## Protected tier

Never removed, never offered for removal:

- The user's known folders, wherever they are redirected (resolved with the Windows Known Folders API): Documents, Desktop, Pictures, Videos, Music.
- Any folder synced by OneDrive or another cloud storage provider.
- `%APPDATA%` (Roaming) entirely.
- `%LOCALAPPDATA%`, except the exact cache paths listed in the Safe tier.
- Browser profiles, except their cache folders listed above.
- `%SystemRoot%`, `%ProgramFiles%`, `%ProgramFiles(x86)%` and `%ProgramData%`, except the exact paths listed in the Safe tier.
- Project folders: any folder containing `.git`, `Cargo.toml`, `package.json`, `*.sln`, `pyproject.toml` or `go.mod`, and everything inside it.
- Database files (`.db`, `.sqlite`, `.mdf`, `.ldf`, `.accdb`) outside the Safe cache paths.
- `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`, `System Volume Information`, the EFI system partition, and recovery partitions.
- Folders the user adds in Settings → Protected folders.
- Anything not matched by a rule.

## Changing these rules

See the safety-rule section of [CONTRIBUTING.md](CONTRIBUTING.md). Every change is listed under **Cleanup rules** in [CHANGELOG.md](CHANGELOG.md).
