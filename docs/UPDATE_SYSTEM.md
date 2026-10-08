# The AllInsight update system

AllInsight works without the network, and updating it is the one exception.
This document covers how the updater is built, what it sends, what it checks,
and what it can and cannot recover from. Each claim names the code it relies
on, so you can check it there.

## Principles

1. **Off until asked.** A new installation never contacts anyone. The default
   setting is *Ask me before checking*: a check happens only when the user
   clicks **Check for Updates**, or at most once per interval if they turn on
   *Automatically check for updates* (`Settings::update_auto_check`, default
   `false`).
2. **Nothing about the user leaves the device.** A check is a plain `GET` of
   one public file that is the same for everyone.
3. **Nothing runs unless it is verified.** The metadata must carry a valid
   signature from the AllInsight update key, and the package must match the
   SHA-256 in that signed metadata. On Windows the installer's Authenticode
   signature is checked as well.
4. **Nothing installs without a click.** An automatic check only reports what
   it found. Downloading takes **Update Now**, and installing takes
   **Restart Now**.
5. **User data is never part of an update.** The database is backed up before
   every install, and the installer never touches it.

## Components

All the update code lives in `src-tauri/src/services/update/`. It imports
nothing from the rest of the services: not the database, the scanner, the
assistant, or the settings document.

| File | Responsibility |
|---|---|
| `config.rs` | The update address, the trusted public key, the required publisher and the allowed hosts. All are fixed at build time and none is a user setting. URL validation (HTTPS only, allowed hosts only, no credentials). |
| `channel.rs` | Release channels (`stable`, `beta`, `dev`; only Stable is offered), update kinds (`application`, `model`, `configuration`, `security`), and the platform key (`windows-x64`, `linux-x64`, `macos-arm64`, …). |
| `version.rs` | Semantic version parsing and comparison with the `semver` crate. Never string comparison. |
| `metadata.rs` | The `latest.json` schema and its validation. |
| `transport.rs` | The only code in AllInsight that makes an internet request. It follows redirects by hand so that every hop is checked, and pins the exact headers sent. |
| `verify.rs` | Signature verification (minisign / `tauri signer`), SHA-256, and Windows Authenticode via `WinVerifyTrust`. |
| `install.rs` | The pending-install record, the hand-over to the installer, the AppImage swap with rollback, and the check after restart. |
| `log.rs` | `logs/updates.log`. |
| `mod.rs` | `UpdateManager`, the state machine the interface drives. |

`src-tauri/src/commands/update.rs` is the IPC layer. It is the only place the
updater meets the rest of the app, and only three things cross there:
- the time of the last successful check
- the user's two update preferences
- a closure that backs up the database

`src/components/Updates.tsx` contains the Settings → Updates section and the
prompt that appears when an automatic check finds an update.

## Build-time configuration

These are environment variables read by the compiler (`option_env!`). Leaving
one unset uses the default shown.

| Variable | Default | Meaning |
|---|---|---|
| `ALLINSIGHT_UPDATE_URL` | `https://github.com/Sk1750-alt/AllInsight/releases/latest/download` | Folder containing `latest.json` and `latest.json.sig`. Must be HTTPS. |
| `ALLINSIGHT_UPDATE_PUBKEY` | Key compiled into `config.rs` | The public half of the update signing key, as printed by `tauri signer generate`. Empty means the updater is disabled. |
| `ALLINSIGHT_UPDATE_PUBLISHER` | unset | Windows only. When set, the installer must carry a valid Authenticode signature whose signer name is exactly this. |
| `ALLINSIGHT_UPDATE_ENABLED` | `true` | `false` builds without the updater. Use this for distribution packages (AUR, Flathub) that update through their package manager. |
| `ALLINSIGHT_UPDATE_EXTRA_HOSTS` | unset | Extra hosts, comma-separated, that downloads may come from. |

These are deliberately not runtime settings. If a settings file could change
the update address or the trusted key, importing someone's settings would
amount to installing whatever they pointed at.

The user-facing preferences are stored in the local settings document and
never anywhere else:
- `update_auto_check`, default `false`
- `update_check_interval_hours`, default 24 and clamped to 24–720

Importing a settings file that turns on automatic checks is flagged as a
weakening change and needs explicit acceptance (`services/config.rs`).

## Metadata: `latest.json`

```json
{
  "schema": 1,
  "product": "AllInsight",
  "channel": "stable",
  "kind": "application",
  "version": "1.0.1",
  "release_date": "2026-10-20",
  "minimum_supported_version": "1.0.0",
  "security": false,
  "release_notes": ["Faster storage scans", "Bug fixes"],
  "installers": {
    "windows-x64": { "url": "https://github.com/…/AllInsight_1.0.1_x64-setup.exe", "sha256": "…", "size": 9437184, "format": "nsis" },
    "linux-x64":   { "url": "https://github.com/…/AllInsight_1.0.1_amd64.AppImage", "sha256": "…", "size": 88000000, "format": "appimage" }
  }
}
```

- `installers` is a map keyed by platform: `windows-x64`, `windows-arm64`,
  `linux-x64`, `macos-x64`, `macos-arm64`, and so on. Adding a platform
  changes no code.
- `format` is one of `nsis`, `appimage`, `deb`, `rpm` or `dmg`.
- `kind` separates application updates from future model, configuration and
  security updates. This version installs `application` and `security`. It
  reports the other kinds but never installs them as an application, so
  changing the interface never forces anyone to download a model again.
- `minimum_supported_version` lets a release require a stepping-stone
  version. Installations older than that are told to download the full
  installer instead.
- Channels other than Stable read `latest-beta.json` or `latest-dev.json`
  from the same folder.

The file is never written by hand: `scripts/make-update-manifest.mjs` builds
it from the files being released. The detached signature
`latest.json.sig` sits next to it.

## Update flow

```text
Check for Updates
  -> GET latest.json, GET latest.json.sig        (HTTPS, allowed hosts only)
  -> verify the signature with the compiled-in key  ── fail ─> "could not be verified"
  -> parse; check product, channel, schema, version
  -> compare versions locally
       same or newer installed ─> "You're up to date."
       newer offered           ─> show version, date, notes  [Update Now] [Later]

Update Now
  -> download to updates/AllInsight-<ver>-<platform>.<ext>.part (with progress)
  -> SHA-256 must equal the signed value         ── fail ─> delete, "verification failed"
  -> Windows: Authenticode must be acceptable    ── fail ─> delete, "verification failed"
  -> rename to the final name  ─> "Update ready."  [Restart Now] [Later]

Restart Now
  -> re-hash the package (it sat on disk)        ── fail ─> delete, "verification failed"
  -> back up the database to backups/pre-update-<from>.db   ── fail ─> stop
  -> write updates/pending.json
  -> Windows:  run the installer with /P /UPDATE /R, then exit
     AppImage: swap the file (keeping .previous), restart into it
     deb/rpm:  show the verified package for the package manager

Next launch
  -> pending.json says which version was attempted
       running it    ─> "Updated to v…", delete the staged package
       running old   ─> "did not finish; v… still installed", keep the package
```

## Version handling

`version.rs` uses `semver` precedence, which gives these results:
- `1.9.0 < 1.10.0`
- `1.4.0-beta.2 < 1.4.0`
- build metadata (`+build.7`) is ignored

A build that is newer than the published version (a developer build) is
reported as up to date and is never offered a downgrade. The running version
is `CARGO_PKG_VERSION`, the same value the About section and the interface
display.

## Automatic checks

`monitor.rs` calls `commands::update::auto_check_if_due` each time it wakes
up. That function does nothing unless every one of these is true:

- the user turned automatic checks on
- this build has an update key, and the updater is enabled
- AllInsight has been running for at least 10 minutes, so launching the app
  never triggers a check
- the interval (24 hours by default) has passed since the last *successful*
  check, stored as `update.last_check`
- at least 6 hours have passed since the last automatic attempt, so being
  offline doesn't cause repeated retries
- no check, download or install is already in progress

An automatic check that finds an update shows a prompt and a notification. One
that fails is silent, because AllInsight works the same offline.

## Offline behaviour

No connection is a normal state, not an error:
- DNS failures, refused connections and timeouts are reported as
  *Unable to check for updates. AllInsight is still fully functional offline.*
  with a **Try Again** button.
- Server errors (5xx, 404) are reported as *Couldn't check for updates. Check
  your Internet connection and try again.*

No other feature depends on the updater, and every network call runs off the
interface thread.

## Recovery: what is and is not rolled back

Here is exactly what each mechanism protects:

- **User data.**
  - The database, settings, history, logs and models live in
    `%LOCALAPPDATA%\AllInsight` or `~/.local/share/AllInsight`, which
    installers do not replace.
  - On Windows, the uninstall hook (`src-tauri/windows/hooks.nsh`) skips data
    removal when `$UpdateMode = 1`, the mode `/UPDATE` selects.
  - The database is also copied to `backups/pre-update-<from>.db` before
    every install. If that copy fails, the install does not start.
- **Database migrations** (`services/db/mod.rs`).
  - Before a newer build changes the schema, the whole file is copied to
    `backups/pre-migration-v<n>.db`.
  - All the steps then run in one SQLite transaction, together with the
    version bump. A failing step rolls everything back, and the database
    stays readable by the version the user had.
- **AppImage.**
  - The new file is copied beside the old one and swapped in with renames.
  - The old file stays as `<name>.previous`, and is put back if the swap
    fails.
- **Windows installer.**
  - NSIS is not transactional. AllInsight cannot undo an installer that is
    killed halfway through.
  - What it guarantees instead is that the installer that runs is the
    verified one, and that this installer stays in `updates/` until the new
    version has started successfully. Running it again repairs the
    installation.
  - The next launch reports an update that did not finish.
- **A download that fails, is interrupted, or fails verification** is
  deleted. It never runs, and nothing about the installed application has
  changed at that point.

## Logging

`logs/updates.log` holds lines such as these:

```text
[2026-10-08 14:20:31] Update check started
[2026-10-08 14:20:32] Metadata received
[2026-10-08 14:20:32] Metadata signature verified
[2026-10-08 14:20:32] Current version: 1.0.0
[2026-10-08 14:20:32] Latest version: 1.0.1
[2026-10-08 14:20:33] Update available
```

The log is written only by the updater, using fixed sentences, version
numbers, error classes and package file names the updater chose itself. The
updater never receives user data, so the log cannot contain any.

## Tests

`cargo test update::` runs the tests below.

- **Versions:** minor and patch steps, a major step, numeric versus text
  comparison, pre-releases, equal versions, no downgrade, malformed input.
- **Network:** offline, timeout, DNS failure, 503, 404, a missing signature,
  invalid JSON, malformed metadata, an HTTP server, an untrusted host, a
  disabled build, and a build with no key (which makes no request at all).
- **Security:**
  - a genuine signature, one changed byte, another key, garbage signatures
  - tampered metadata, a wrong SHA-256, a corrupted download
  - a package altered after download and before install
  - an HTTP installer URL, an untrusted installer host, look-alike hosts,
    credentials in the URL, redirect checks
  - required-publisher rules, and an unsigned file recognised as unsigned on
    Windows
- **Scenarios:**
  - installed version equal to, older than, newer than, and below the
    minimum of the published one
  - download and verify
  - an interrupted download leaves nothing behind
  - a failed backup stops the install
  - a completed or interrupted install is recognised after restart
  - an AppImage swap and its rollback
- **Database:**
  - a successful migration backs up first and keeps the data
  - a failing migration rolls back completely
  - an older database is upgraded when it is reopened
  - a backup copy can be read
- **Privacy:**
  - a check makes exactly two requests, to the two fixed URLs, with only the
    `User-Agent: AllInsight-Updater` header
  - no request contains the user name, computer name, home folder, data
    folder, platform or installed version
