#!/usr/bin/env node
// Build and optionally publish the exact Windows release checked out in this repository.
import { createReadStream, existsSync, readFileSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const flags = new Set(process.argv.slice(2));
const supported = new Set(["--metadata", "--check", "--publish", "--verify-install"]);
if ([...flags].some(flag => !supported.has(flag))) {
  throw new Error("Usage: node scripts/build-installer.mjs [--metadata] [--check] [--publish] [--verify-install]");
}

function execute(program, args, options = {}) {
  const result = spawnSync(program, args, { cwd: root, stdio: "inherit", ...options });
  if (result.error) throw new Error(`${program}: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`${program} failed (exit ${result.status ?? "unknown"}).`);
}

function capture(program, args) {
  const result = spawnSync(program, args, { cwd: root, encoding: "utf8" });
  if (result.error || result.status !== 0) {
    throw new Error(`${program} ${args.join(" ")} failed: ${result.error?.message ?? result.stderr?.trim() ?? result.status}`);
  }
  return result.stdout.trim();
}

function npm(...args) {
  // On Windows npm.cmd is a batch file, so launch it through cmd.exe.
  execute(process.env.ComSpec || "cmd.exe", ["/d", "/c", "npm.cmd", ...args]);
}

const packageVersion = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
const tauriVersion = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8")).version;
const cargo = readFileSync(join(root, "src-tauri", "Cargo.toml"), "utf8");
const packageSection = cargo.split(/^\[package\]\s*$/m)[1]?.split(/^\[/m)[0];
const cargoVersion = packageSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!/^\d+\.\d+\.\d+$/.test(packageVersion) || packageVersion !== tauriVersion || packageVersion !== cargoVersion) {
  throw new Error(`Version mismatch: package.json=${packageVersion}, tauri.conf.json=${tauriVersion}, Cargo.toml=${cargoVersion}`);
}
const version = packageVersion;
const tag = `v${version}`;
console.log(`TAILOR ${version} — ${tag}`);
if (flags.has("--metadata")) process.exit(0);
const hasGit = existsSync(join(root, ".git"));
let commit = null;
if (hasGit) {
  commit = capture("git", ["rev-parse", "HEAD"]);
  const dirty = capture("git", ["status", "--porcelain", "--untracked-files=no"]);
  if (dirty) throw new Error("Tracked project files have local changes. Commit or review them before building a release.");
  console.log(`Source commit: ${commit}`);
} else {
  if (flags.has("--publish")) throw new Error("Publishing requires a Git checkout. Build this copied source locally without --publish.");
  console.log("Copied source folder without Git; building the version shown above locally.");
}
if (flags.has("--check")) process.exit(0);
if (process.platform !== "win32") throw new Error("Build the Windows installer on a Windows computer.");

if (flags.has("--publish")) {
  execute("gh", ["auth", "status"]);
  const remoteMain = capture("git", ["ls-remote", "origin", "refs/heads/main"]).split(/\s+/)[0];
  if (commit !== remoteMain) throw new Error(`Local HEAD ${commit} differs from origin/main ${remoteMain}. Update the checkout before publishing.`);
}

npm("install");
npm("run", "build");
npm("run", "test:measurements");
npm("run", "test:license");
execute("cargo", ["test", "--manifest-path", "src-tauri/Cargo.toml", "--release"]);
npm("run", "tauri", "build");

const executable = join(root, "src-tauri", "target", "release", "tailor-workspace.exe");
if (!existsSync(executable)) throw new Error(`Release executable missing: ${executable}`);
const pe = readFileSync(executable);
const header = pe.readUInt32LE(0x3c);
if (pe.toString("ascii", 0, 2) !== "MZ" || pe.toString("ascii", header, header + 4) !== "PE\0\0" || pe.readUInt16LE(header + 24 + 68) !== 2) {
  throw new Error("Release executable is not a Windows GUI application.");
}

const installer = join(root, "src-tauri", "target", "release", "bundle", "nsis", `TAILOR_${version}_x64-setup.exe`);
if (!existsSync(installer) || statSync(installer).size < 1_000_000) {
  throw new Error(`Expected installer was not built: ${installer}`);
}
if (flags.has("--verify-install")) execute(installer, ["/S"]);
const hash = createHash("sha256");
for await (const chunk of createReadStream(installer)) hash.update(chunk);
console.log(`Installer: ${installer}`);
console.log(`SHA-256: ${hash.digest("hex")}`);

if (flags.has("--publish")) {
  const existing = spawnSync("gh", ["release", "view", tag], { cwd: root, stdio: "ignore" });
  if (existing.error) throw new Error(`gh: ${existing.error.message}`);
  if (existing.status === 0) throw new Error(`${tag} already has a release; refusing to replace its installer.`);
  const remoteTag = capture("git", ["ls-remote", "origin", `refs/tags/${tag}^{}`]);
  if (remoteTag && remoteTag.split(/\s+/)[0] !== commit) throw new Error(`${tag} points to a different commit on GitHub.`);
  execute("gh", ["release", "create", tag, installer, "--target", commit, "--title", `TAILOR ${version}`, "--generate-notes"]);
  console.log(`Published: https://github.com/Tiji10584/TAILOR-reference/releases/tag/${tag}`);
}
