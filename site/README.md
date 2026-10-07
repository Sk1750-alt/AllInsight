# AllInsight landing site

Static HTML, CSS and JavaScript. No build step, no framework, no external
requests. Geist and Geist Mono (SIL OFL, licence beside them) are served
from `assets/fonts/`. Nginx serves this directory as-is.

```
site/
  index.html            home
  features.html         the fourteen screens, with four live demos
  privacy.html          what is kept, what never is, offline
  safety.html           the deletion guard and its eight checks
  ai.html               the local assistant and what it is given
  about.html            architecture, engineering, licence, contact
  download.html         downloads, requirements, adding a local model
  404.html              not found
  robots.txt, sitemap.xml, <key>.txt (IndexNow)
  styles.css            all styling, tokens lifted from src/index.css
  app.js                animation and behaviour for every page, no dependencies
  downloads.json        version, file sizes and checksums (generated)
  update-downloads.ps1  regenerates downloads.json from dist-release
  assets/               app icon, favicons and the bundled font
```

## Pages

Every page carries the same header, phone menu and footer, written out in
full in each file, so a change to the navigation is a change to all eight.
Links use clean paths (`/features`, not `/features.html`); nginx maps them
with `try_files $uri $uri.html $uri/ =404` and serves `404.html` through
`error_page 404 /404.html`.

The moving parts in `app.js` each look for their own markup and do nothing
where it is absent: the background field (`canvas.field`, light or dark),
counting numbers (`data-count`), the treemap, health ring, live chart,
duplicate pipeline and assistant demos on Features and AI, and the safety
rail. All of them pause off screen and in background tabs, and
`prefers-reduced-motion` shows each in its finished state.

## Local preview

```bash
cd site
python -m http.server 8787 --bind 127.0.0.1
```

Then open <http://127.0.0.1:8787/>. Python's server does not know the
clean paths, so add `.html` by hand (`/features.html`) when previewing this
way; nginx needs no such help.

## Visuals

Dark, quiet and precise: a near-black ground (#08090a), hairline borders,
and one teal light. Geist for everything, Geist Mono for small technical
labels. Amber is kept for the dot in the mark and for "safe to clean".

Every page header sits on the same light: a slow teal aurora, a dot grid
that fades at the edges, and thin beams that run along the grid's rows and
columns (`LIGHT` in the page generator; dots sit at 50% ± 32n across and
16 + 32n down, so the beams are placed on them).

The home page opens on the app's Overview screen in its dark theme, drawn in
HTML with a glowing edge, which straightens from a tilt as it is scrolled to.

Motion, all in `app.js` and `styles.css`:

- headlines settle out of a soft blur, word by word, on load
- the announcement pill has a light turning around its border
- cards (`.tile`, `.screen`, `.layer`, `.platform`, `.window`, steps) carry
  a light that follows the pointer around their border, on hover devices
- long sentences (`.highlight`) light up word by word as they are read
- `.blackout` statements brighten and sharpen out of the dark on scroll
- tiles, stats and lists rise out of a slight blur; counters count
- the Features and AI demos: treemap, health ring, live chart, duplicate
  pipeline, assistant conversation

`prefers-reduced-motion` shows every piece in its finished state.
`assets/og-image.png` is rendered from `../assets/social/og-image.svg`.

`styles.css` and `app.js` are linked with a `?v=` query. Bump it when either
changes, because nginx lets browsers cache them for an hour.

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

The one continuous animation, the privacy section's point of light, runs
only while it is on screen. `prefers-reduced-motion:
reduce` shows every visual in its finished state with no motion at all.
`downloads.json` fills in the version, sizes and hashes, including the
optional `linux` entries.
