import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const files = ["scripts/configure.mjs", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/src/main.rs", "package.json", "index.html", "starter.config.json", ".github/workflows/release.yml", "README.md"];
function fixture(t) {
  const dir = mkdtempSync(resolve(tmpdir(), "starter-configure-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const file of files) {
    mkdirSync(dirname(resolve(dir, file)), { recursive: true });
    copyFileSync(resolve(root, file), resolve(dir, file));
  }
  const read = (file) => readFileSync(resolve(dir, file), "utf8");
  const json = (file) => JSON.parse(read(file));
  const run = (...args) => spawnSync(process.execPath, [resolve(dir, "scripts/configure.mjs"), ...args], { encoding: "utf8" });
  const features = () => JSON.parse(read("src-tauri/Cargo.toml").match(/^default = (\[[^\n]*\])/m)[1]);
  return { dir, read, json, run, features };
}

test("database-only setup preserves identity and excludes the unselected engine", (t) => {
  const f = fixture(t);
  const identity = [f.json("package.json").name, f.json("src-tauri/tauri.conf.json").identifier, f.read("src-tauri/src/main.rs")];
  for (const engine of ["sqlcipher", "turso", "sqlcipher"]) {
    const result = f.run("--database", engine);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(f.json("starter.config.json").database.engine, engine);
    assert.equal(f.json("starter.config.json").database.importSqlcipher, false);
    assert.ok(f.features().includes(`database-${engine}`));
    assert.ok(!f.features().includes(`database-${engine === "turso" ? "sqlcipher" : "turso"}`));
    assert.ok(!f.features().includes("legacy-sqlcipher-import"));
    assert.deepEqual([f.json("package.json").name, f.json("src-tauri/tauri.conf.json").identifier, f.read("src-tauri/src/main.rs")], identity);
  }
});

test("legacy import is explicit, persists on identity changes, and can be removed", (t) => {
  const f = fixture(t);
  assert.equal(f.run("--database", "turso", "--import-sqlcipher", "true").status, 0);
  assert.ok(f.features().includes("legacy-sqlcipher-import"));
  assert.equal(f.run("--name", "Acme Notes", "--identifier", "com.acme.notes").status, 0);
  assert.equal(f.json("starter.config.json").database.engine, "turso");
  assert.equal(f.json("starter.config.json").database.importSqlcipher, true);
  assert.ok(f.read("src-tauri/src/main.rs").includes("acme_notes_lib::run()"));
  assert.equal(f.run("--database", "turso", "--import-sqlcipher", "false").status, 0);
  assert.ok(!f.features().includes("legacy-sqlcipher-import"));
});

test("invalid choices and help never partially rewrite a project", (t) => {
  const f = fixture(t);
  const before = files.map(f.read);
  for (const args of [["--database", "mysql"], ["--database", "sqlcipher", "--import-sqlcipher", "true"], ["--database", "turso", "--import-sqlcipher", "maybe"], ["--database", "turso", "--feature", "missing=true"]]) {
    assert.notEqual(f.run(...args).status, 0);
    assert.deepEqual(files.map(f.read), before);
  }
  assert.equal(f.run("--help").status, 0);
  assert.deepEqual(files.map(f.read), before);
});

test("repeated database selection is idempotent and preserves telemetry features", (t) => {
  const f = fixture(t);
  assert.equal(f.run("--database", "turso", "--import-sqlcipher", "false").status, 0);
  const before = files.map(f.read);
  assert.equal(f.run("--database", "turso").status, 0);
  assert.deepEqual(files.map(f.read), before);
  assert.ok(f.features().includes("telemetry-http"));
  assert.ok(f.features().includes("telemetry-sentry"));
});
