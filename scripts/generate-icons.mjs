/**
 * Rasterises the AllInsight mark and hands the result to the Tauri icon generator,
 * which produces the full Windows set (.ico with 16/24/32/48/64/256, plus the
 * PNGs Tauri embeds). The SVG stays the source of truth; nothing in
 * src-tauri/icons is edited by hand.
 *
 * The brand kit draws the mark differently at 32 px and below (no i-stem, a
 * heavier stroke, a larger dot), so after Tauri has written its set, the
 * 16/24/32 px entries of icon.ico and 32x32.png are replaced with renders of
 * allinsight-app-icon-small.svg. The tray uses the window icon, so it gets the
 * small mark too.
 *
 * Usage: node scripts/generate-icons.mjs
 */
import { execFileSync } from "node:child_process";
import { mkdirSync, existsSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(root, "assets/logo/allinsight-app-icon.svg");
const smallSource = resolve(root, "assets/logo/allinsight-app-icon-small.svg");
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

console.log("Swapping in the small mark at 16, 24 and 32 px...");
const render = (svg, size) =>
  sharp(svg, { density: 1200 }).resize(size, size).png({ compressionLevel: 9 }).toBuffer();

// An .ico is a directory of images; Windows Vista and later accept PNG
// payloads, which keeps the alpha edge of the rounded tile clean.
const sizes = [16, 24, 32, 48, 64, 256];
const images = await Promise.all(sizes.map((n) => render(n <= 32 ? smallSource : source, n)));
const header = Buffer.alloc(6 + 16 * sizes.length);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(sizes.length, 4);
let offset = header.length;
sizes.forEach((n, i) => {
  const entry = 6 + 16 * i;
  header.writeUInt8(n % 256, entry);
  header.writeUInt8(n % 256, entry + 1);
  header.writeUInt16LE(1, entry + 4);
  header.writeUInt16LE(32, entry + 6);
  header.writeUInt32LE(images[i].length, entry + 8);
  header.writeUInt32LE(offset, entry + 12);
  offset += images[i].length;
});
writeFileSync(resolve(root, "src-tauri/icons/icon.ico"), Buffer.concat([header, ...images]));
writeFileSync(resolve(root, "src-tauri/icons/32x32.png"), images[2]);
writeFileSync(resolve(root, "src-tauri/icons/Square30x30Logo.png"), await render(smallSource, 30));

console.log("Icons written to src-tauri/icons.");
