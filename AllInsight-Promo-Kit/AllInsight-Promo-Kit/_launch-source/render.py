"""Render the launch film and posters with headless Chromium.

  python render.py stills 2 6 9 12 17 19     # check frames -> out/still-*.png
  python render.py film                       # 20 s, 1920x1080, 30 fps H.264 (FPS=60 for smoother)
  python render.py posters                    # every poster -> 05-launch-posters

Needs: playwright, imageio-ffmpeg (pip), and a Chromium on PATH.
"""
import os, subprocess, sys, pathlib, shutil
from playwright.sync_api import sync_playwright
import imageio_ffmpeg

HERE = pathlib.Path(__file__).resolve().parent
KIT = HERE.parent
CHROME = shutil.which("chromium") or shutil.which("google-chrome")
FPS, LENGTH = int(os.environ.get("FPS", 30)), 20.0


def page_for(pw, w, h, scale=1):
    b = pw.chromium.launch(executable_path=CHROME, args=["--no-sandbox", "--disable-dev-shm-usage", "--font-render-hinting=none", "--disable-extensions", "--disable-background-networking", "--renderer-process-limit=1", "--disable-features=Translate,MediaRouter,OptimizationHints", "--js-flags=--max-old-space-size=96"], env={**os.environ, "DBUS_SESSION_BUS_ADDRESS": "/dev/null"}, timeout=180000)
    p = b.new_page(viewport={"width": w, "height": h}, device_scale_factor=scale)
    return b, p


def film(stills=None):
    with sync_playwright() as pw:
        b, p = page_for(pw, 1920, 1080)
        p.goto((HERE / "film.html").as_uri() + "?render=1")
        p.evaluate("document.fonts.ready.then(() => 1)")
        p.wait_for_timeout(300)
        if stills:
            (HERE / "out").mkdir(exist_ok=True)
            for t in stills:
                p.evaluate(f"render({t})")
                p.screenshot(path=str(HERE / "out" / f"still-{t}.png"))
            return b.close()
        # Frames go to disk first (resumable on a slow machine), then one encode.
        frames = pathlib.Path(os.environ.get("FRAMEDIR", HERE / "frames")); frames.mkdir(exist_ok=True)
        n = int(FPS * LENGTH)
        import time; t0 = time.time()
        budget = int(os.environ.get("LIMIT", n))  # render at most this many new frames per run
        for i in range(n):
            f = frames / f"{i:05d}.jpg"
            if f.exists():
                continue
            if budget == 0:
                print("paused at", i, flush=True); return b.close()
            budget -= 1
            p.evaluate(f"render({i / FPS})")
            f.write_bytes(p.screenshot(type="jpeg", quality=96))
            if i % 30 == 0:
                print(f"frame {i}/{n}  {time.time() - t0:.0f}s", flush=True)
        b.close()
        out = KIT / "04-launch-film" / "allinsight-launch-film-16x9.mp4"
        subprocess.run([imageio_ffmpeg.get_ffmpeg_exe(), "-y", "-loglevel", "error",
            "-framerate", str(FPS), "-i", str(frames / "%05d.jpg"),
            "-c:v", "libx264", "-preset", "faster", "-crf", "18", "-pix_fmt", "yuv420p",
            "-tune", "film", "-movflags", "+faststart", str(out)], check=True)
        print("wrote", out)


def posters():
    specs = [  # id, width, height, file name
        ("hero", 1080, 1350, "allinsight-poster-understood-1080x1350"),
        ("zero", 1080, 1350, "allinsight-poster-zero-1080x1350"),
        ("dot", 1080, 1350, "allinsight-poster-the-dot-1080x1350"),
        ("hero", 1080, 1920, "allinsight-poster-understood-story-1080x1920"),
        ("zero", 1080, 1920, "allinsight-poster-zero-story-1080x1920"),
        ("dot", 1080, 1920, "allinsight-poster-the-dot-story-1080x1920"),
    ]
    outdir = KIT / "05-launch-posters"
    with sync_playwright() as pw:
        only = os.environ.get("ONLY")
        for pid, w, h, name in specs:
            if only and only not in name:
                continue
            b, p = page_for(pw, w, h, scale=2)
            p.goto((HERE / "posters.html").as_uri() + f"?p={pid}&w={w}&h={h}")
            p.evaluate("document.fonts.ready.then(() => 1)")
            p.wait_for_timeout(300)
            p.screenshot(path=str(outdir / f"{name}.png"))
            b.close()
            print("wrote", name)


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "stills":
        film([float(x) for x in sys.argv[2:]])
    elif cmd == "film":
        film()
    elif cmd == "posters":
        posters()
