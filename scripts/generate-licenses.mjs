/**
 * Collects the licence texts of every third-party package AllInsight ships and
 * writes them to one file.
 *
 * MIT, Apache-2.0 and the other permissive licences in the dependency tree all
 * require their text to travel with a distributed binary. The output goes to
 * src/generated/, where Settings > About imports it as a code-split module, so
 * both the installer and the portable executable carry it and it is only loaded
 * when someone opens it. It is imported rather than fetched because the
 * application's content security policy allows no fetch of its own assets, and
 * widening that policy is not worth it for one text file. A copy goes to site/
 * for the download page.
 *
 * Works offline and adds no dependency: it reads what cargo and npm already
 * have on disk. It errs on the side of listing too much. A few build-time tools
 * are included, which costs nothing; missing a shipped package would not be.
 *
 * Usage: node scripts/generate-licenses.mjs
 */
import { execSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = join(root, "src-tauri");
const TARGET = "x86_64-pc-windows-msvc";

const LICENCE_FILE = /^(licen[cs]e|copying|notice|unlicense)([-._].*)?$/i;

function run(command, args, cwd) {
  // Run through the shell because npm is a .cmd shim on Windows, which Node
  // will not launch directly. Every argument here is a constant in this file,
  // so joining them is safe. npm ls exits non-zero for extraneous or unmet peer
  // packages while still printing a usable list, so a failure with output is
  // accepted and only an empty result is fatal.
  try {
    return execSync([command, ...args].join(" "), {
      cwd,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      stdio: ["ignore", "pipe", "ignore"],
    });
  } catch (error) {
    if (error.stdout && error.stdout.trim()) return error.stdout;
    throw new Error(`${command} ${args.join(" ")} failed: ${error.message}`);
  }
}

function licenceTexts(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && LICENCE_FILE.test(entry.name))
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((entry) => readFileSync(join(dir, entry.name), "utf8").replace(/\r\n/g, "\n").trim())
    .filter(Boolean);
}

// --- Rust ------------------------------------------------------------------

function rustPackages() {
  // Only normal dependencies for the Windows target: build scripts, dev
  // dependencies and other platforms' crates never reach the binary.
  const shipped = new Set(
    run(
      "cargo",
      ["tree", "--offline", "-e", "normal", "--target", TARGET, "--prefix", "none", "-f", "{p}"],
      tauriDir,
    )
      .split(/\r?\n/)
      .map((line) => line.replace(/ \(\*\)$/, "").trim())
      .filter(Boolean)
      .map((line) => {
        const [name, version] = line.split(" ");
        return `${name} ${version?.replace(/^v/, "")}`;
      }),
  );

  const metadata = JSON.parse(
    run("cargo", ["metadata", "--offline", "--format-version", "1", "--filter-platform", TARGET], tauriDir),
  );

  return metadata.packages
    .filter((p) => shipped.has(`${p.name} ${p.version}`) && p.name !== "allinsight")
    .map((p) => ({
      ecosystem: "Rust",
      name: p.name,
      version: p.version,
      licence: p.license ?? (p.license_file ? "see licence file" : "not declared"),
      texts: licenceTexts(dirname(p.manifest_path)),
    }));
}

// --- npm -------------------------------------------------------------------

function npmPackages() {
  return run("npm", ["ls", "--omit=dev", "--all", "--parseable"], root)
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((dir) => dir && resolve(dir) !== root && existsSync(join(dir, "package.json")))
    .map((dir) => {
      const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
      const legacy = (pkg.licenses ?? []).map((l) => l.type ?? l).join(" OR ");
      const licence =
        typeof pkg.license === "string" ? pkg.license : (pkg.license?.type ?? (legacy || "not declared"));
      return { ecosystem: "npm", name: pkg.name, version: pkg.version, licence, texts: licenceTexts(dir) };
    });
}

// --- Output ----------------------------------------------------------------

function unique(packages) {
  const seen = new Map();
  for (const p of packages) seen.set(`${p.ecosystem}:${p.name}@${p.version}`, p);
  return [...seen.values()].sort(
    (a, b) => a.ecosystem.localeCompare(b.ecosystem) || a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
  );
}

function render(packages) {
  // Packages that ship identical licence text are grouped under it once. The
  // windows-* crates alone would otherwise repeat the same two texts dozens of
  // times.
  const groups = new Map();
  const withoutText = [];
  for (const p of packages) {
    if (p.texts.length === 0) {
      withoutText.push(p);
      continue;
    }
    const key = p.texts.join("\n\n-----\n\n").replace(/\s+/g, " ");
    if (!groups.has(key)) groups.set(key, { texts: p.texts, members: [] });
    groups.get(key).members.push(p);
  }

  const line = (p) => `  ${p.name} ${p.version} (${p.ecosystem}, ${p.licence})`;
  const rule = "=".repeat(78);
  const out = [
    "AllInsight - third-party software licences",
    "",
    "AllInsight includes the open-source packages listed below. Each is used under",
    "the licence shown with it. AllInsight's own licence is in LICENSE.",
    "",
    `${packages.length} packages. Generated by scripts/generate-licenses.mjs.`,
    "",
  ];

  for (const group of [...groups.values()].sort((a, b) => a.members[0].name.localeCompare(b.members[0].name))) {
    out.push(rule, "", "The following packages are provided under the licence text below:", "");
    out.push(...group.members.map(line), "");
    out.push(...group.texts.flatMap((text) => [text, ""]));
  }

  if (withoutText.length > 0) {
    out.push(
      rule,
      "",
      "The following packages declare a licence but do not ship a licence file.",
      "They are used under the licence each one declares:",
      "",
      ...withoutText.map(line),
      "",
    );
  }

  return out.join("\n");
}

const packages = unique([...rustPackages(), ...npmPackages()]);
const text = render(packages);

mkdirSync(join(root, "src", "generated"), { recursive: true });
for (const target of [
  join(root, "src", "generated", "third-party-licenses.txt"),
  join(root, "site", "third-party-licenses.txt"),
]) {
  writeFileSync(target, text, "utf8");
}

const missing = packages.filter((p) => p.texts.length === 0).length;
console.log(
  `Wrote third-party-licenses.txt: ${packages.length} packages, ` +
    `${(text.length / 1024).toFixed(0)} KB, ${missing} without a licence file.`,
);
