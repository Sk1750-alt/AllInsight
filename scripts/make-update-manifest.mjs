#!/usr/bin/env node
/**
 * Build latest.json, the release metadata the AllInsight updater reads.
 *
 *   node scripts/make-update-manifest.mjs \
 *     --version 1.0.1 \
 *     --asset windows-x64=release-files/AllInsight_1.0.1_x64-setup.exe:nsis \
 *     --asset linux-x64=release-files/AllInsight_1.0.1_amd64.AppImage:appimage \
 *     [--min-version 1.0.0] [--security] [--out latest.json]
 *
 * The SHA-256 and size of every asset are computed here, from the files that
 * will be uploaded, never typed by hand. Release notes are the bullet points
 * under the version's heading in CHANGELOG.md. Download URLs point at the
 * GitHub release for tag v<version>.
 *
 * Sign the result afterwards; the updater ignores an unsigned file:
 *
 *   npx tauri signer sign -f <your update.key> latest.json
 *
 * See docs/RELEASE_PROCESS.md.
 */
import { createHash } from "node:crypto";
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";

const REPO = "Sk1750-alt/AllInsight";
const FORMATS = new Set(["nsis", "appimage", "deb", "rpm", "dmg"]);
const PLATFORM = /^(windows|linux|macos)-(x64|arm64)$/;

function fail(message) {
  console.error(`make-update-manifest: ${message}`);
  process.exit(1);
}

const args = process.argv.slice(2);
const options = { assets: [], security: false, out: "latest.json", channel: "stable" };
for (let i = 0; i < args.length; i++) {
  const flag = args[i];
  const value = () => args[++i] ?? fail(`${flag} needs a value`);
  if (flag === "--version") options.version = value();
  else if (flag === "--min-version") options.min = value();
  else if (flag === "--asset") options.assets.push(value());
  else if (flag === "--out") options.out = value();
  else if (flag === "--channel") options.channel = value();
  else if (flag === "--date") options.date = value();
  else if (flag === "--security") options.security = true;
  else fail(`unknown option ${flag}`);
}

const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;
if (!options.version || !SEMVER.test(options.version)) fail("--version must be like 1.0.1");
if (options.min && !SEMVER.test(options.min)) fail("--min-version must be like 1.0.0");
if (!["stable", "beta", "dev"].includes(options.channel)) fail("--channel is stable, beta or dev");
if (options.assets.length === 0) fail("give at least one --asset platform=path:format");

// The version must match what the application itself reports, or every
// installation of it would be offered itself as an update forever.
const cargo = readFileSync("src-tauri/Cargo.toml", "utf8").match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (cargo !== options.version) {
  fail(`--version ${options.version} does not match src-tauri/Cargo.toml (${cargo})`);
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

const installers = {};
for (const spec of options.assets) {
  const match = spec.match(/^([^=]+)=(.+):([a-z]+)$/);
  if (!match) fail(`--asset ${spec} must be platform=path:format`);
  const [, platform, path, format] = match;
  if (!PLATFORM.test(platform)) fail(`unknown platform ${platform}`);
  if (!FORMATS.has(format)) fail(`unknown format ${format}`);
  const file = resolve(path);
  const name = basename(file);
  if (/\s/.test(name)) fail(`${name}: asset names must not contain spaces`);
  installers[platform] = {
    url: `https://github.com/${REPO}/releases/download/v${options.version}/${encodeURIComponent(name)}`,
    sha256: sha256(file),
    size: statSync(file).size,
    format,
  };
}

function releaseNotes(version) {
  const changelog = readFileSync("CHANGELOG.md", "utf8").split(/\r?\n/);
  const start = changelog.findIndex((line) => line.startsWith(`## [${version}]`));
  if (start < 0) fail(`CHANGELOG.md has no "## [${version}]" section`);
  const notes = [];
  for (const line of changelog.slice(start + 1)) {
    if (line.startsWith("## ")) break;
    const bullet = line.match(/^- (.+)$/);
    if (bullet) {
      notes.push(
        bullet[1]
          .replace(/\*\*(.+?)\*\*/g, "$1")
          .replace(/`([^`]+)`/g, "$1")
          .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1"),
      );
    }
  }
  return notes.slice(0, 20);
}

const manifest = {
  schema: 1,
  product: "AllInsight",
  channel: options.channel,
  kind: options.security ? "security" : "application",
  version: options.version,
  release_date: options.date ?? new Date().toISOString().slice(0, 10),
  minimum_supported_version: options.min ?? "1.0.0",
  security: options.security,
  release_notes: releaseNotes(options.version),
  installers,
};

const file = options.channel === "stable" ? options.out : options.out.replace(/latest\.json$/, `latest-${options.channel}.json`);
writeFileSync(file, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Wrote ${file} for AllInsight ${options.version}:`);
for (const [platform, entry] of Object.entries(installers)) {
  console.log(`  ${platform.padEnd(12)} ${entry.sha256}  ${entry.size} bytes`);
}
console.log(`\nNext: npx tauri signer sign -f <path to update.key> ${file}`);
