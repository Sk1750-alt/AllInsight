# AllInsight landing site

Static HTML, CSS and JavaScript. No build step, no framework, no external
requests — not even a webfont. Nginx serves this directory as-is.

```
site/
  index.html            the page
  styles.css            all styling, tokens lifted from src/index.css
  app.js                animation and behaviour, no dependencies
  downloads.json        version, file sizes and checksums (generated)
  update-downloads.ps1  regenerates downloads.json from dist-release
  assets/               logo and favicon
  screens/              product screenshots
```

## Local preview

```bash
cd site
python -m http.server 8787 --bind 127.0.0.1
```

Then open <http://127.0.0.1:8787/>.

## Screenshots

`screens/` holds three shots copied from `shots/`, which is gitignored. They
were picked deliberately:

| File | Source | Why this one |
| --- | --- | --- |
| `overview.png` | `shots/overview.png` | The health score with its reasons. No user paths. |
| `performance.png` | `shots/performance.png` | Live metrics. No user paths. |
| `processes.png` | `shots/processes.png` | Publisher, cost and uptime per process. No user paths. |

**Check any shot you add.** Several in `shots/` are not publishable as-is:

- `assistant.png` prints the model folder as a full path, which contains the
  **Windows username** (`C:\Users\<name>\AppData\Local\AllInsight\engine`)
- `drive-health.png` exposes the drive **serial number**
- `storage-map.png`, `large-files.png` and `duplicates.png` show real folder
  and file names from the machine that captured them
- `cleanup.png` is safe but catches the category rows mid-load, so it reads as
  a set of empty grey bars

Redact before publishing, or leave them out. A page whose whole argument is
"nothing leaves your machine" cannot be the thing that publishes the author's
username.

### Recapturing

```powershell
powershell -ExecutionPolicy Bypass -File scripts\capture-all.ps1
```

It writes all fourteen screens to `shots/`. Two things to know:

- It drives navigation by writing `ui.last_route` into the settings database,
  so the app must have run once and finished its first-run wizard. On a fresh
  profile every capture is otherwise the welcome screen.
- `scripts\click-through-firstrun.ps1` completes that wizard, but `SendKeys`
  fails with `Access is denied` when the shell and the app run at different
  integrity levels. Failing that, set the flag directly: write
  `{"first_run_complete": true}` to the `settings.v1` key in
  `%LOCALAPPDATA%\AllInsight\allinsight.db`. `Settings` is `#[serde(default)]`,
  so a partial blob fills in the rest.

## Before every deploy

Run this after the release build, from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File site\update-downloads.ps1
```

It reads `dist-release/` and writes real sizes and SHA-256 hashes into
`downloads.json`. The page fetches that file at load and fills in the version,
file names, sizes and checksums. When a value is `null` the markup's own
fallback text stands, so a missing or stale `downloads.json` degrades to
"— publish after the release build —" rather than to a wrong hash.

A published hash that does not match the served binary is worse than no hash.
The installer is unsigned, so the checksum is the only way a visitor can
verify what they downloaded.

## Deploying to the VM

```bash
sudo git -C /opt/allinsight pull
sudo rsync -a --delete /opt/allinsight/site/ /var/www/allinsight/ \
  --exclude downloads \
  --exclude '.*' \
  --exclude README.md \
  --exclude update-downloads.ps1
```

`--exclude downloads` keeps the binaries from being wiped. `--exclude '.*'`
matters more than it looks: tooling drops state directories into whatever
directory it runs in, and some of them contain local tokens. Nothing starting
with a dot belongs in a web root.

The `.exe` files live in `/var/www/allinsight/downloads/` and are **not** in this
repository — they come from a GitHub Release or a direct `scp`. Keeping them
out of git is deliberate: `AllInsight.exe` alone is 10 MB and changes every build.

The nginx server block sets `Content-Disposition: attachment` on
`/downloads/`, which is why the buttons point at relative `/downloads/...`
paths rather than at GitHub. If you change the buttons to absolute GitHub
URLs, that nginx rule stops applying.

## Notes on the JavaScript

Every continuous animation — the hero treemap, the packet field — is gated on
an `IntersectionObserver` and on `document.hidden`, so nothing runs while it is
off screen or in a background tab. `prefers-reduced-motion: reduce` replaces
both canvases with a single static frame and disables the reveals, the
scramble, and the count-ups.
