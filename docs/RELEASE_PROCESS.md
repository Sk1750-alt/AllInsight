# Releasing AllInsight

This is the checklist for every release. Steps 1–3 are one-time setup; steps
4–9 repeat for each version. For how the updater consumes what you publish,
see [UPDATE_SYSTEM.md](UPDATE_SYSTEM.md).

---

## One-time setup

### 1. The update signing key

This key is what lets installed copies of AllInsight trust an update. If you
lose it, no existing installation can ever be updated again. If it leaks,
someone else can sign updates.

1. Generate the key pair with a strong password:

   ```powershell
   mkdir $env:USERPROFILE\.allinsight
   npx tauri signer generate -w "$env:USERPROFILE\.allinsight\update.key"
   ```

2. Put the printed **public** key into `DEFAULT_PUBLIC_KEY` in
   `src-tauri/src/services/update/config.rs`. It is public and is meant to be
   committed. Every build after this one trusts it.
3. Back up `update.key` and its password in two places, for example a password
   manager and an offline USB drive. **Never commit the private key.**
4. Give CI the key so it can sign `latest.json`:

   ```powershell
   gh secret set TAURI_SIGNING_PRIVATE_KEY < "$env:USERPROFILE\.allinsight\update.key"
   gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD
   ```

To replace a key later, ship a release signed with the **old** key whose
binary trusts the **new** key, and keep the old key until most users have
moved onto that release.

### 2. Code signing (Windows)

The updater is secure without it, because the signed `latest.json` and its
SHA-256 values are the gate. Code signing only removes the SmartScreen
*Unknown publisher* warning. Once you have a certificate:

1. Sign the installer in CI. Tauri reads `bundle.windows.certificateThumbprint`
   or a custom `signCommand` in `tauri.conf.json`.
2. Build with `ALLINSIGHT_UPDATE_PUBLISHER` set to the signer name exactly as
   it appears on the certificate. From then on, updates signed by anyone else,
   or not signed at all, are refused.

Set the publisher **only after** a signed release is already out. Otherwise
the release that turns the requirement on would refuse its own unsigned
successor.

See [Certificates, licences and permissions](#certificates-licences-and-permissions)
for the free and low-cost options.

### 3. The update server

The default needs no server: GitHub serves `releases/latest/download/<file>`
from the newest published non-prerelease. To use your own domain later:

1. Host `latest.json` and `latest.json.sig` over HTTPS at that domain.
2. Build with `ALLINSIGHT_UPDATE_URL=https://updates.example/stable`.
3. Add any download host the metadata points at to
   `ALLINSIGHT_UPDATE_EXTRA_HOSTS`.

Keep serving the GitHub location as well for as long as old builds use it.

---

## Every release

### 4. Bump the version in all three places

- `package.json` → `"version"`
- `src-tauri/Cargo.toml` → `version`
- `src-tauri/tauri.conf.json` → `"version"`

Add a `## [x.y.z] — YYYY-MM-DD` section to `CHANGELOG.md`. Its bullet points
become the "What's new" list users see.

If the release changes the database schema, add a `Migration` to
`MIGRATIONS` in `src-tauri/src/services/db/mod.rs`, raise `SCHEMA_VERSION`,
and never edit a migration that has already shipped.

### 5. Test locally

```powershell
npm run typecheck
cd src-tauri; cargo test; cd ..
```

### 6. Tag and push

```powershell
git commit -am "Release x.y.z"
git tag vX.Y.Z
git push origin main vX.Y.Z
```

CI (`.github/workflows/build.yml`) then does the following:
- builds and tests on Windows, Linux and macOS
- keeps the packages
- builds `latest.json` from those exact files with
  `scripts/make-update-manifest.mjs` (SHA-256 and sizes are computed, never
  typed)
- signs it with the `TAURI_SIGNING_PRIVATE_KEY` secret
- opens a **draft** release containing the installers, `latest.json`,
  `latest.json.sig` and `SHA256SUMS.txt`

### 7. Check the draft

1. Open the draft on GitHub, Releases, and confirm all files are attached.
2. Download `latest.json` and check that the version and notes are right.
3. Download `latest.json` and `latest.json.sig` next to each other and check
   them against the key the app trusts. This is the same check the app makes:

   ```powershell
   node scripts/verify-update-manifest.mjs latest.json latest.json.sig
   ```

   It must print `OK: ... signed by the AllInsight update key`.

### 7a. If CI did not sign (no `TAURI_SIGNING_PRIVATE_KEY` secret)

The draft then has `latest.json` but no `latest.json.sig`. Sign it on your own
machine, where the key lives, and attach the signature:

```powershell
gh release download vX.Y.Z -p latest.json --clobber
npx tauri signer sign -f "$env:USERPROFILE\.allinsight\allinsight.key" latest.json
node scripts/verify-update-manifest.mjs latest.json latest.json.sig
gh release upload vX.Y.Z latest.json.sig
```

Signing locally keeps the private key off GitHub entirely, at the cost of one
manual step per release.

### 8. Publish

Click **Publish release**. From this moment,
`releases/latest/download/latest.json` points at the new version, and every
installation that checks is offered it.

### 9. Test the update from the previous version

On a clean Windows machine or VM:

1. Install the **previous** version and use it a little (run a scan, change a
   setting).
2. Go to Settings → Updates → Check for Updates. It should offer the new
   version.
3. Click Update Now. It downloads and verifies, and shows "Update ready".
4. Click Restart Now. The installer runs and AllInsight restarts.
5. Confirm that the new version shows, the settings and history are intact,
   and `logs/updates.log` reads *Update to x.y.z completed*.

Repeat on Linux with the AppImage.

### Releasing by hand (if CI is unavailable)

```powershell
node scripts/make-update-manifest.mjs --version X.Y.Z `
  --asset "windows-x64=release-files/AllInsight_X.Y.Z_x64-setup.exe:nsis" `
  --asset "linux-x64=release-files/AllInsight_X.Y.Z_amd64.AppImage:appimage"
npx tauri signer sign -f "$env:USERPROFILE\.allinsight\update.key" latest.json
node scripts/verify-update-manifest.mjs
gh release create vX.Y.Z release-files/* latest.json latest.json.sig --title "AllInsight X.Y.Z" --notes-file CHANGELOG.md
```

### Pulling a bad release

1. Publish a fixed version with a higher number. Updates never go backwards,
   so this is the only fix that reaches people who already updated.
2. To stop more people installing the bad version in the meantime, mark that
   release as a pre-release on GitHub. `latest/download` then points back at
   the previous release. Users already on the bad version see "up to date"
   until the fix ships.

---

## Certificates, licences and permissions

What AllInsight needs to be distributed legitimately on Windows and Linux, and
which of it is free:

| Item | Needed? | Cost | How |
|---|---|---|---|
| Source licence | Yes, done | Free | MIT (`LICENSE`). Third-party licences ship in the app (Settings → About). |
| Update signing key | Yes | Free | Step 1 above. |
| GitHub Releases hosting | Yes | Free | Public repository. |
| Privacy policy | Yes, done | Free | `PRIVACY.md`, plus a public URL (the GitHub file is fine). Required by the Microsoft Store and Flathub. |
| Windows code signing | Recommended | Free for OSS via SignPath Foundation | See below. |
| Linux signing | Optional | Free | GPG, see below. |

**Windows code-signing options:**
- **SignPath Foundation.** Free code signing for open-source projects. You
  apply with the public repository and builds must come from CI (GitHub
  Actions qualifies). The certificate is issued to SignPath Foundation, which
  is therefore the publisher name shown and the value for
  `ALLINSIGHT_UPDATE_PUBLISHER`. Check the current requirements at
  signpath.org.
- **Certum Open Source Code Signing.** A low-cost certificate (not free) in
  your own name, for open-source developers.
- **Azure Artifact Signing** (formerly Trusted Signing). A monthly
  subscription. Eligibility for individual developers depends on your
  country, so check Microsoft's current list.
- **Microsoft Store.** Developer registration is free for individuals. Store
  listings need the privacy policy URL. Win32 installers submitted to the
  Store must still be signed by a trusted certificate.
- **winget.** A free listing, made through a pull request to
  `microsoft/winget-pkgs` that points at the GitHub release URL and SHA-256.

**Linux:**
- No certificate is required to distribute on Linux.
- GPG-sign `SHA256SUMS.txt` (`gpg --detach-sign --armor SHA256SUMS.txt`) and
  publish your public key so people can verify downloads.
- **AUR:** free. `packaging/arch/PKGBUILD` already exists. Distribution
  packages update through their package manager.
- **Flathub:** free, after review. Build its package with
  `ALLINSIGHT_UPDATE_ENABLED=false`.

**Permissions AllInsight needs at run time** (no special licence required):
- Windows: a per-user install with no administrator rights. "Restart as
  administrator" is optional and only adds full drive health and machine-wide
  startup entries.
- Linux: an ordinary user. Drive health reads UDisks2 over the system bus,
  where polkit decides what an unprivileged user may read.
- Network: only what's described in [PRIVACY.md](PRIVACY.md).
