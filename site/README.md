# AllInsight landing site

Static HTML, CSS and JavaScript. No build step, no framework, no external
requests. The one webfont, Instrument Sans (SIL OFL, licence beside it), is
served from `assets/fonts/`. Nginx serves this directory as-is.

```
site/
  index.html            the page
  styles.css            all styling, tokens lifted from src/index.css
  app.js                animation and behaviour, no dependencies
  downloads.json        version, file sizes and checksums (generated)
  update-downloads.ps1  regenerates downloads.json from dist-release
  assets/               logo, favicon and the bundled font
  screens/              product screenshots (WebP, title bar cropped)
```

## Local preview

```bash
cd site
python -m http.server 8787 --bind 127.0.0.1
```

Then open <http://127.0.0.1:8787/>.

## Visuals

The page has no screenshots. Every visual is drawn in HTML, CSS and SVG and
animated by `app.js`: the hero health dial, the packets that stop at the wall
in the dark privacy section, the three-scene "How it works" story (a sticky
stage that changes with the step nearest the middle of the viewport), the
count-up numbers, the living feature tiles, and the file that travels the
eight safety checks as the page scrolls. Figures shown in them are
illustrative and match the kind of reading the app gives.

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

Every continuous animation (the packets, the processor trace, the duplicate
files, the marquee) runs only while it is on screen. `prefers-reduced-motion:
reduce` shows every visual in its finished state with no motion at all.
`downloads.json` fills in the version, sizes and hashes, including the
optional `linux` entries.
