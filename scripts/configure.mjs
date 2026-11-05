#!/usr/bin/env node
// Configure the template for YOUR app in one go. Idempotent: run it again to
// change anything. Rewrites the identity in every file that carries it so
// nothing is left saying "Hello Desktop".
//
//   npm run configure -- --name "Acme Notes" --identifier com.acme.notes \
//     [--org "Acme Inc"] [--homepage https://acme.com] [--repo acme/notes] \
//     [--releases-repo acme/notes-releases] [--updater-pubkey "<minisign pubkey>"] \
//     [--icon ./brand/icon.png] [--feature tray=false --feature autostart=false] \
//     [--database sqlcipher|turso] [--import-sqlcipher true|false]
//     [--locales en,es] [--required-permissions screen_recording,accessibility]
//
// Files touched: src-tauri/tauri.conf.json, src-tauri/Cargo.toml, src-tauri/src/main.rs,
// package.json, index.html, starter.config.json, .github/workflows/release.yml,
// README.md (title only). Icons are regenerated with `tauri icon` when --icon is given.

import { execSync } from "node:child_process";
import { createInterface } from "node:readline/promises";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = parseArgs(process.argv.slice(2));

const confPath = resolve(root, "src-tauri/tauri.conf.json");
const conf = readJson(confPath);
const scPath = resolve(root, "starter.config.json");
const sc = readJson(scPath);
const savedDatabase = sc.database ?? { engine: "sqlcipher", importSqlcipher: false };

if (args.help) {
  showHelp();
  process.exit(0);
}
if (process.stdin.isTTY && process.stdout.isTTY) {
  const prompt = createInterface({ input: process.stdin, output: process.stdout });
  try {
    if (!args.name && !args.database) {
      args.name = (await prompt.question(`App name [${conf.productName}]: `)).trim() || conf.productName;
      args.identifier = (await prompt.question(`Bundle identifier [${conf.identifier}]: `)).trim() || conf.identifier;
    }
    if (!args.database) {
      console.log("\nDatabase choice:\n  1. SQLCipher — established encrypted storage (default)\n  2. Turso — native async + exact vector search; experimental encryption");
      const answer = (await prompt.question(`Choose 1 or 2 [${savedDatabase.engine === "turso" ? "2" : "1"}]: `)).trim();
      args.database = answer === "1" ? "sqlcipher" : answer === "2" ? "turso" : answer || savedDatabase.engine;
    }
    if (args.database === "turso" && args["import-sqlcipher"] === undefined) {
      const defaultImport = savedDatabase.engine === "turso" && savedDatabase.importSqlcipher;
      const answer = (await prompt.question(`Import an existing SQLCipher install? Adds SQLCipher dependencies [${defaultImport ? "Y/n" : "y/N"}]: `)).trim().toLowerCase();
      if (!["", "y", "yes", "n", "no"].includes(answer)) die("answer yes or no for SQLCipher import");
      args["import-sqlcipher"] = answer === "" ? defaultImport : ["y", "yes"].includes(answer);
    }
  } finally { prompt.close(); }
}
if (!args.name && !args.database && args["import-sqlcipher"] === undefined) {
  showHelp();
  process.exit(1);
}
const engine = args.database ?? savedDatabase.engine;
if (!["sqlcipher", "turso"].includes(engine)) die("--database must be sqlcipher or turso");
const importArg = args["import-sqlcipher"];
if (importArg !== undefined && ![true, false, "true", "false"].includes(importArg)) die("--import-sqlcipher must be true or false");
const importSqlcipher = importArg === undefined
  ? (engine === "turso" && savedDatabase.engine === "turso" && savedDatabase.importSqlcipher)
  : importArg === true || importArg === "true";
if (importSqlcipher && engine !== "turso") die("--import-sqlcipher requires --database turso");
sc.database = { ...sc.database, engine, importSqlcipher: Boolean(importSqlcipher) };
// Validate choices before rewriting any files.
for (const f of toList(args.feature)) {
  const [k] = f.split("=");
  if (!(k in sc.features)) die(`unknown feature "${k}" (have: ${Object.keys(sc.features).join(", ")})`);
}

const name = String(args.name ?? conf.productName).trim();
const slug = args.name ? toSlug(name) : readJson(resolve(root, "package.json")).name; // package.json name, keychain items, updater key file
const originalCargo = readFileSync(resolve(root, "src-tauri/Cargo.toml"), "utf8");
const oldCrate = originalCargo.match(/^name = "([^"]+)"/m)[1];
const oldLib = originalCargo.match(/^name = "([^"]+_lib)"/m)[1];
const crate = args.name ? slug : oldCrate;
const crateLib = args.name ? `${slug.replaceAll("-", "_")}_lib` : oldLib;
const identifier = args.identifier ? String(args.identifier) : null;

// ---------------------------------------------------------------- tauri.conf.json
const previousName = conf.productName;
if (args.name) {
  conf.productName = name;
  conf.app.windows[0].title = name;
}
if (identifier) conf.identifier = identifier;
if (args["releases-repo"]) {
  conf.plugins ??= {};
  conf.plugins.updater ??= {};
  conf.plugins.updater.endpoints = [
    `https://github.com/${args["releases-repo"]}/releases/latest/download/latest.json`,
  ];
}
if (args["updater-pubkey"]) {
  conf.plugins.updater.pubkey = String(args["updater-pubkey"]);
  conf.bundle.createUpdaterArtifacts = true;
}
writeJson(confPath, conf);

// ---------------------------------------------------------------- Cargo.toml + main.rs
const cargoPath = resolve(root, "src-tauri/Cargo.toml");
let cargo = originalCargo;
cargo = cargo.replace(/^name = "[^"]+"/m, `name = "${crate}"`);
cargo = cargo.replace(/^name = "[^"]+_lib"/m, `name = "${crateLib}"`);
if (args.name) cargo = cargo.replace(/^description = "[^"]*"/m, `description = "${name}"`);
if (args.org) cargo = cargo.replace(/^authors = \[[^\]]*\]/m, `authors = ["${args.org}"]`);
const defaultFeatures = JSON.parse(cargo.match(/^default = (\[[^\n]*\])/m)[1])
  .filter((feature) => !["database-sqlcipher", "database-turso", "legacy-sqlcipher-import"].includes(feature));
defaultFeatures.push(`database-${engine}`);
if (importSqlcipher) defaultFeatures.push("legacy-sqlcipher-import");
cargo = cargo.replace(/^default = \[[^\n]*\]/m, `default = ${JSON.stringify(defaultFeatures)}`);
writeFileSync(cargoPath, cargo);
replaceInFile(resolve(root, "src-tauri/src/main.rs"), `${oldLib}::run()`, `${crateLib}::run()`);

// ---------------------------------------------------------------- package.json, index.html
const pkgPath = resolve(root, "package.json");
const pkg = readJson(pkgPath);
pkg.name = slug;
writeJson(pkgPath, pkg);
if (args.name) replaceInFile(resolve(root, "index.html"), /<title>.*<\/title>/, `<title>${escapeHtml(name)}</title>`);

// ---------------------------------------------------------------- starter.config.json
if (args.org) sc.app.org = String(args.org);
if (args.homepage) sc.app.homepage = String(args.homepage);
if (args.repo) {
  sc.app.supportUrl = `https://github.com/${args.repo}/issues`;
  sc.app.docsUrl = `https://github.com/${args.repo}#readme`;
}
if (args["updater-pubkey"]) sc.features.updater = true;
for (const f of toList(args.feature)) {
  const [k, v] = f.split("=");
  if (!(k in sc.features)) die(`unknown feature "${k}" (have: ${Object.keys(sc.features).join(", ")})`);
  sc.features[k] = v !== "false";
}
if (args.locales) sc.i18n.supported = toList(args.locales);
if (args["required-permissions"]) sc.permissions.required = toList(args["required-permissions"]);
if (args["optional-permissions"]) sc.permissions.optional = toList(args["optional-permissions"]);
writeJson(scPath, sc);

// ---------------------------------------------------------------- CI release workflow
const relPath = resolve(root, ".github/workflows/release.yml");
if (existsSync(relPath) && args["releases-repo"]) {
  const [owner, repo] = String(args["releases-repo"]).split("/");
  let yml = readFileSync(relPath, "utf8");
  yml = yml.replace(/owner: .*/, `owner: ${owner}`).replace(/repo: .*-releases.*/, `repo: ${repo}`);
  yml = yml.replaceAll(/releaseName: ".*\$\{\{ github.ref_name \}\}"/g, `releaseName: "${name} \${{ github.ref_name }}"`);
  writeFileSync(relPath, yml);
}

// ---------------------------------------------------------------- README title
if (args.name) replaceInFile(resolve(root, "README.md"), new RegExp(`^# ${escapeRegex(previousName)}`, "m"), `# ${name}`);

// ---------------------------------------------------------------- icons
if (args.icon) {
  const icon = resolve(process.cwd(), String(args.icon));
  if (!existsSync(icon)) die(`icon not found: ${icon}`);
  console.log("→ generating icons from", icon);
  execSync(`npx tauri icon "${icon}"`, { cwd: root, stdio: "inherit" });
}

console.log(`
✓ Configured "${name}"
  identifier      ${conf.identifier}
  package / crate ${slug} (lib ${crateLib}, was ${oldCrate})
  database        ${engine} (vectors: ${engine === "turso" ? "enabled" : "unavailable"}, SQLCipher import: ${importSqlcipher ? "enabled" : "disabled"})
  updater         ${sc.features.updater ? "enabled" : "disabled"}${conf.plugins?.updater?.endpoints ? ` → ${conf.plugins.updater.endpoints[0]}` : ""}
  features        ${Object.entries(sc.features).map(([k, v]) => `${k}=${v}`).join(" ")}

Next: npm install && npm run tauri dev
Release setup: docs/RELEASING.md
`);

// ---------------------------------------------------------------- helpers
function showHelp() {
  console.log(`Configure the template (run without arguments in a terminal for prompts).

npm run configure -- --name "Acme Notes" --identifier com.acme.notes
  --database sqlcipher|turso         SQLCipher is the default; Turso enables vectors
  --import-sqlcipher true|false      Turso only; false for new projects
  --org "Acme Inc" --homepage https://acme.com --repo acme/notes
  --releases-repo acme/notes-releases --updater-pubkey "<public key>"
  --icon ./brand/icon.png --feature tray=false --feature autostart=false
  --locales en,es --required-permissions screen_recording,accessibility

Database-only selection preserves the app identity:
  npm run configure -- --database turso

Non-interactive calls keep the saved database choice unless specified.
Database formats differ. See docs/DATABASE.md before changing an existing app.`);
}

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) continue;
    const key = a.slice(2);
    const next = argv[i + 1];
    if (next === undefined || next.startsWith("--")) {
      out[key] = true;
    } else if (key === "feature") {
      (out.feature ??= []).push(next);
      i++;
    } else {
      out[key] = next;
      i++;
    }
  }
  return out;
}
function toList(v) {
  if (!v) return [];
  return (Array.isArray(v) ? v : String(v).split(",")).map((s) => s.trim()).filter(Boolean);
}
function toSlug(s) {
  return s
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "app";
}
function readJson(p) {
  return JSON.parse(readFileSync(p, "utf8"));
}
function writeJson(p, v) {
  if (JSON.stringify(readJson(p)) === JSON.stringify(v)) return;
  writeFileSync(p, JSON.stringify(v, null, 2) + "\n");
}
function replaceInFile(p, from, to) {
  if (!existsSync(p)) return;
  const s = readFileSync(p, "utf8");
  const next = s.replace(from, to);
  if (next !== s) writeFileSync(p, next);
}
function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
}
function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
function die(msg) {
  console.error("error:", msg);
  process.exit(1);
}
