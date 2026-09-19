/**
 * Rasterises the AllInsight mark and hands the result to the Tauri icon generator,
 * which produces the full Windows set (.ico with 16/24/32/48/64/256, plus the
 * PNGs Tauri embeds). The SVG stays the source of truth; nothing in
 * src-tauri/icons is edited by hand.
 *
 * Usage: node scripts/generate-icons.mjs
 */
import { execFileSync } from "node:child_process";
import { mkdirSync, existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(root, "assets/logo/allinsight-app-icon.svg");
const buildDir = resolve(root, "assets/icons");
const raster = resolve(buildDir, "allinsight-1024.png");

if (!existsSync(source)) {
  console.error(`Icon source not found: ${source}`);
  process.exit(1);
}

mkdirSync(buildDir, { recursive: true });

console.log("Rasterising the mark at 1024x1024...");
await sharp(source, { density: 600 })
  .resize(1024, 1024, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } })
  .png({ compressionLevel: 9 })
  .toFile(raster);

// A flat favicon for the web layer, which never sees the .ico.
await sharp(source, { density: 600 })
  .resize(256, 256)
  .png()
  .toFile(resolve(buildDir, "allinsight-256.png"));

console.log("Generating the Windows icon set...");
// `shell: true` is required on Windows, where npx is a .cmd shim that
// CreateProcess refuses to launch directly.
execFileSync("npx", ["tauri", "icon", raster, "--output", "src-tauri/icons"], {
  cwd: root,
  stdio: "inherit",
  shell: true,
});

console.log("Icons written to src-tauri/icons.");
