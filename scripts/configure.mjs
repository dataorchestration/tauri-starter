#!/usr/bin/env node
// Configure the template for YOUR app in one go. Idempotent: run it again to
// change anything. Rewrites the identity in every file that carries it so
// nothing is left saying "Hello Desktop".
//
//   npm run configure -- --name "Acme Notes" --identifier com.acme.notes \
//     [--org "Acme Inc"] [--homepage https://acme.com] [--repo acme/notes] \
//     [--releases-repo acme/notes-releases] [--updater-pubkey "<minisign pubkey>"] \
//     [--icon ./brand/icon.png] [--feature tray=false --feature autostart=false] \
//     [--locales en,es] [--required-permissions screen_recording,accessibility]
//
// Files touched: src-tauri/tauri.conf.json, src-tauri/Cargo.toml, src-tauri/src/main.rs,
// package.json, index.html, starter.config.json, .github/workflows/release.yml,
// README.md (title only). Icons are regenerated with `tauri icon` when --icon is given.

import { execSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = parseArgs(process.argv.slice(2));

if (args.help || !args.name) {
  console.log(readFileSync(fileURLToPath(import.meta.url), "utf8").split("\n").slice(1, 15).join("\n").replaceAll("// ", ""));
  process.exit(args.help ? 0 : 1);
}

const name = String(args.name).trim();
const slug = toSlug(name); // package.json name, keychain items, updater key file
const crate = slug; // Cargo package name (kebab)
const crateLib = `${slug.replaceAll("-", "_")}_lib`;
const identifier = args.identifier ? String(args.identifier) : null;

// ---------------------------------------------------------------- tauri.conf.json
const confPath = resolve(root, "src-tauri/tauri.conf.json");
const conf = readJson(confPath);
const previousName = conf.productName;
conf.productName = name;
if (identifier) conf.identifier = identifier;
conf.app.windows[0].title = name;
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
let cargo = readFileSync(cargoPath, "utf8");
const oldCrate = cargo.match(/^name = "([^"]+)"/m)[1];
const oldLib = cargo.match(/^name = "([^"]+_lib)"/m)[1];
cargo = cargo.replace(/^name = "[^"]+"/m, `name = "${crate}"`);
cargo = cargo.replace(/^name = "[^"]+_lib"/m, `name = "${crateLib}"`);
cargo = cargo.replace(/^description = "[^"]*"/m, `description = "${name}"`);
if (args.org) cargo = cargo.replace(/^authors = \[[^\]]*\]/m, `authors = ["${args.org}"]`);
writeFileSync(cargoPath, cargo);
replaceInFile(resolve(root, "src-tauri/src/main.rs"), `${oldLib}::run()`, `${crateLib}::run()`);

// ---------------------------------------------------------------- package.json, index.html
const pkgPath = resolve(root, "package.json");
const pkg = readJson(pkgPath);
pkg.name = slug;
writeJson(pkgPath, pkg);
replaceInFile(resolve(root, "index.html"), /<title>.*<\/title>/, `<title>${escapeHtml(name)}</title>`);

// ---------------------------------------------------------------- starter.config.json
const scPath = resolve(root, "starter.config.json");
const sc = readJson(scPath);
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
replaceInFile(resolve(root, "README.md"), new RegExp(`^# ${escapeRegex(previousName)}`, "m"), `# ${name}`);

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
  updater         ${sc.features.updater ? "enabled" : "disabled"}${conf.plugins?.updater?.endpoints ? ` → ${conf.plugins.updater.endpoints[0]}` : ""}
  features        ${Object.entries(sc.features).map(([k, v]) => `${k}=${v}`).join(" ")}

Next: npm install && npm run tauri dev
Release setup: docs/RELEASING.md
`);

// ---------------------------------------------------------------- helpers
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
