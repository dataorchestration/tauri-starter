#!/usr/bin/env node
// Publish a release to the public releases repo the updater polls:
// collects every bundle + .sig under src-tauri/target, builds latest.json for
// all platforms found, and creates a GitHub release (via `gh`) with the assets.
//
// The releases repo is read from tauri.conf.json → plugins.updater.endpoints.
// Keeping it separate from the source repo lets the updater read it without
// auth while the source stays private.
//
//   node scripts/publish-feed.mjs --version 1.2.0 --notes "What changed" [--draft] [--dry-run]
//
// Requires: gh (authenticated with push access to the releases repo).

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = Object.fromEntries(
  process.argv.slice(2).map((a, i, all) => (a.startsWith("--") ? [a.slice(2), all[i + 1]?.startsWith("--") || all[i + 1] === undefined ? true : all[i + 1]] : [])).filter((x) => x.length),
);
const version = args.version;
if (!version) die("--version is required");
const notes = args.notes ?? `Release ${version}`;

const conf = JSON.parse(readFileSync(resolve(root, "src-tauri/tauri.conf.json"), "utf8"));
const endpoint = conf.plugins?.updater?.endpoints?.[0] ?? "";
const m = endpoint.match(/github\.com\/([^/]+\/[^/]+)\/releases\/latest\/download\//);
if (!m) die(`cannot derive the releases repo from updater endpoint "${endpoint}"`);
const releasesRepo = m[1];
const baseUrl = `https://github.com/${releasesRepo}/releases/latest/download/`;
const productName = conf.productName;

// Updater artifact per platform key → glob-ish matcher under target/.
const targets = [
  { key: "darwin-aarch64", test: (p) => p.endsWith(".app.tar.gz") && p.includes("universal-apple-darwin") },
  { key: "darwin-x86_64", test: (p) => p.endsWith(".app.tar.gz") && p.includes("universal-apple-darwin") },
  { key: "darwin-aarch64", test: (p) => p.endsWith(".app.tar.gz") && p.includes("aarch64-apple-darwin") },
  { key: "darwin-x86_64", test: (p) => p.endsWith(".app.tar.gz") && p.includes("x86_64-apple-darwin") },
  { key: "windows-x86_64", test: (p) => /-setup\.exe$/.test(p) && p.includes("nsis") },
  { key: "linux-x86_64", test: (p) => p.endsWith(".AppImage") },
];

const files = walk(resolve(root, "src-tauri/target")).filter((p) => p.includes("/bundle/"));
const assets = new Set();
const platforms = {};
for (const t of targets) {
  const artifact = files.find(t.test);
  if (!artifact || platforms[t.key]) continue;
  const sig = `${artifact}.sig`;
  if (!existsSync(sig)) {
    console.warn(`! ${basename(artifact)} has no .sig — skipping ${t.key} in latest.json (updater disabled?)`);
    continue;
  }
  platforms[t.key] = { signature: readFileSync(sig, "utf8").trim(), url: baseUrl + encodeURIComponent(basename(artifact)) };
  assets.add(artifact);
  assets.add(sig);
}
// Installers for humans.
for (const p of files) if (/\.(dmg|msi|exe|deb|rpm|AppImage)$/.test(p)) assets.add(p);

const latest = { version, notes, pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"), platforms };
const tmp = mkdtempSync(join(tmpdir(), "feed-"));
const latestPath = join(tmp, "latest.json");
writeFileSync(latestPath, JSON.stringify(latest, null, 2) + "\n");

console.log(`→ ${releasesRepo} v${version}`);
console.log(`  platforms in latest.json: ${Object.keys(platforms).join(", ") || "(none)"}`);
for (const a of assets) console.log(`  asset ${basename(a)} (${(statSync(a).size / 1e6).toFixed(1)} MB)`);

if (args["dry-run"]) {
  console.log(readFileSync(latestPath, "utf8"));
  process.exit(0);
}
if (Object.keys(platforms).length === 0 && !args["allow-no-updater"]) {
  die("no updater artifacts found; pass --allow-no-updater to publish installers only");
}

const ghArgs = ["release", "create", `v${version}`, "-R", releasesRepo, "--title", `${productName} ${version}`, "--notes", notes];
if (args.draft) ghArgs.push("--draft");
if (Object.keys(platforms).length) ghArgs.push(latestPath);
ghArgs.push(...assets);
execFileSync("gh", ghArgs, { stdio: "inherit" });
console.log(`✓ published https://github.com/${releasesRepo}/releases/tag/v${version}`);

function walk(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = join(dir, e.name);
    return e.isDirectory() ? walk(p) : [p];
  });
}
function die(msg) {
  console.error("error:", msg);
  process.exit(1);
}
