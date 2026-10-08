#!/usr/bin/env node
/**
 * Verify latest.json against latest.json.sig with the public key compiled
 * into AllInsight, exactly as the updater will.
 *
 *   node scripts/verify-update-manifest.mjs [latest.json] [latest.json.sig]
 *
 * Exits non-zero if the signature, the key id, or the trusted comment does
 * not verify. Run it on the files attached to a draft release before
 * publishing it.
 */
import { createHash, createPublicKey, verify } from "node:crypto";
import { readFileSync } from "node:fs";

const [docPath = "latest.json", sigPath = `${docPath}.sig`] = process.argv.slice(2);

function fail(message) {
  console.error(`verify-update-manifest: ${message}`);
  process.exit(1);
}

/** Accept minisign text or the base64 of it that `tauri signer` writes. */
function minisignText(input) {
  const trimmed = input.trim();
  if (trimmed.startsWith("untrusted comment:")) return trimmed;
  const decoded = Buffer.from(trimmed.replace(/\s+/g, ""), "base64").toString("utf8");
  if (!decoded.startsWith("untrusted comment:")) fail("not a minisign key or signature");
  return decoded.trim();
}

const config = readFileSync("src-tauri/src/services/update/config.rs", "utf8");
const keySource =
  process.env.ALLINSIGHT_UPDATE_PUBKEY ??
  config.match(/const DEFAULT_PUBLIC_KEY: &str =\s*"([^"]*)"/)?.[1];
if (!keySource) fail("no public key in config.rs or ALLINSIGHT_UPDATE_PUBKEY");

const keyLine = minisignText(keySource).split("\n")[1];
const key = Buffer.from(keyLine, "base64");
if (key.length !== 42 || key.subarray(0, 2).toString() !== "Ed") fail("malformed public key");
const keyId = key.subarray(2, 10);
const publicKey = createPublicKey({
  key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), key.subarray(10)]),
  format: "der",
  type: "spki",
});

const lines = minisignText(readFileSync(sigPath, "utf8")).split("\n");
const signature = Buffer.from(lines[1], "base64");
const trusted = lines[2]?.replace(/^trusted comment: /, "");
const globalSignature = Buffer.from(lines[3] ?? "", "base64");
if (signature.length !== 74) fail("malformed signature");

const algorithm = signature.subarray(0, 2).toString();
if (!signature.subarray(2, 10).equals(keyId)) {
  fail("signed with a different key than the one AllInsight trusts");
}

const document = readFileSync(docPath);
const message =
  algorithm === "ED" ? createHash("blake2b512").update(document).digest() : document;
if (!verify(null, message, publicKey, signature.subarray(10))) {
  fail("SIGNATURE DOES NOT MATCH: the file was changed after signing, or signed with another key");
}
if (
  trusted === undefined ||
  !verify(null, Buffer.concat([signature.subarray(10), Buffer.from(trusted)]), publicKey, globalSignature)
) {
  fail("the trusted comment does not verify");
}

const manifest = JSON.parse(document.toString("utf8"));
console.log(`OK: ${docPath} is signed by the AllInsight update key.`);
console.log(`    version ${manifest.version}, channel ${manifest.channel}, platforms ${Object.keys(manifest.installers).join(", ")}`);
