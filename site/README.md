# AllInsight landing site

Static HTML, CSS and JavaScript. No build step, no framework, no external
requests. The two webfonts, Sora and Instrument Serif Italic (both SIL OFL,
licences beside them), are served from `assets/fonts/`. Nginx serves this
directory as-is.

```
site/
  index.html            the page
  styles.css            all styling, tokens lifted from src/index.css
  app.js                animation and behaviour, no dependencies
  downloads.json        version, file sizes and checksums (generated)
  update-downloads.ps1  regenerates downloads.json from dist-release
  assets/               app icon, favicons and the bundled font
```

## Local preview

```bash
cd site
python -m http.server 8787 --bind 127.0.0.1
```

Then open <http://127.0.0.1:8787/>.

## Visuals

The page follows the brand kit: Paper ground with a faint drawn grain, Ink
text, Graphite for the privacy section, Sora Light headlines, hairline rules
instead of cards, and Insight Amber only for the dot in the mark and small
status marks. Instrument Serif Italic is the single accent: a few words per
headline, the method numerals and the zero, never body text.

`styles.css` and `app.js` are linked with a `?v=` query. Bump it when either
changes, because nginx lets browsers cache them for an hour.

The logo is the A·i mark (an A with a lowercase i inside it) and the
lowercase wordmark, whose two i's are dotless with an amber dot set in CSS.
Both are inline SVG and HTML, so nothing extra loads. Three animations, all
CSS: the mark draws itself on load (the A, then the i, then the dot drops and
sends out two scan rings, in the order of the promo kit's logo reveal); the
app icon builds itself when the download section arrives; and one amber point
of light stops at the wall in the privacy section. `prefers-reduced-motion`
shows each in its finished state.

`assets/allinsight.svg`, `favicon.ico` and `apple-touch-icon.png` are the
brand kit's app icon: Slate tile, full mark above 32 px, the heavier small
mark at 16 and 32 px.

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
