# AllInsight Promo Kit

## 01-logo-animations
All videos are 30 fps H.264 MP4 on Graphite (#0F1115). Transparent versions are WebM (VP9 with alpha) for laying over your own footage in DaVinci Resolve, CapCut, After Effects or Premiere (with the WebM plug-in).

| Folder | Length | What happens | Formats |
|---|---|---|---|
| 01-logo-reveal | 5.5 s | The A draws itself, the i rises, the amber dot drops in, then slides into the full lockup with the tagline. Main intro/outro. | 16:9, 9:16, 1:1, transparent 16:9 |
| 02-insight-pulse | 6 s | The amber dot appears alone and sends out scan rings; the A forms around it; "Your device, understood." | 16:9, 9:16 |
| 03-app-icon-sting | 2.5 s | The Windows app icon builds itself. Use at the end of demos or as a sticker. | 1:1, transparent 1:1 |
| 04-storage-to-insight | 8 s | A storage gauge fills to 82%, shows 14.7 GB to reclaim, then collapses into the dot and becomes the logo. Product story in one shot. | 16:9, 9:16 |
| 05-tagline-reel | 8 s | "Your storage. Your drive health. Your performance. Understood." then the logo. Made for Reels/Shorts. | 9:16, 16:9 |
| 06-coming-soon-teaser | 6 s | The dot blinks like a standby light, the A forms, "Coming soon". | 9:16, 1:1 |

## 02-posters
Each poster is a PNG plus an editable SVG (all text outlined, no fonts needed).
- 01-coming-soon: feed 1080×1350, story 1080×1920
- 02-launch: feed 1080×1350, story 1080×1920
- 03-product-poster: A3 portrait, print-ready PDF + 300 dpi PNG + 1080 px preview

Before you post the launch poster, add your download link or website. The numbers in the app illustration are sample values.

## 03-website
`index.html` is a single self-contained demo page. Open it in any browser.
The app window in the hero is interactive (Overview, Storage map with drill-down, Drive health, AI chat), as are the cleanup groups, the cleanup dialog, the FAQ and the early-access form (front-end only; connect it to your form backend or email tool).
Fonts load from Google Fonts: Sora and IBM Plex Mono.

## 04-launch-film
`allinsight-launch-film-16x9.mp4`: 20 s, 1920×1080, 30 fps H.264, no audio (add your own track).

| Time | What happens |
|---|---|
| 0–3.5 s | An amber dot glows in the dark. "Your device knows everything." |
| 3.5–5.5 s | "Now, *so do you.*" |
| 5.5–11 s | The Overview window rises out of the dark and settles flat; the health ring counts to 92, the bars and storage map fill, a light sweeps the glass. "Your whole device. *In one calm view.*" |
| 11–15.5 s | 14.7 GB found safe to clean · Zero accounts, trackers or cloud · 100% on your device |
| 15.5–20 s | The mark draws itself, then "AllInsight", "Your device, *understood.*" and the footer. |

## 05-launch-posters
PNG at 2× (2160×2700 for feed, 2160×3840 for story).
- **understood**: the app window lit from below, "Your device, understood."
- **zero**: Paper ground, a serif "0" with the amber dot, "bytes leave your device."
- **the-dot**: the amber dot alone, "See everything. Send nothing."

The numbers in the app window and the 14.7 GB are sample values.

## _launch-source
The film and posters are HTML (`film.html`, `posters.html`, shared pieces in `shared.js`) rendered by `render.py` with headless Chromium and ffmpeg (`pip install playwright imageio-ffmpeg`). Open `film.html` in a browser to preview it in real time; `film.html?t=12` freezes one moment. `python render.py film` resumes from frames already in `frames/`, so delete that folder after changing the film.
