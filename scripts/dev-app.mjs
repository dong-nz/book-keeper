#!/usr/bin/env node
// Dev launcher (user request 2026-11-19): runs `tauri dev` with a data
// directory separate from the installed application's library. The mechanism
// is the existing test-only override BOOKKEEPER_DATA_DIR (src-tauri/src/
// data_dir.rs, DR-162) — no product code, nothing shipped as a feature.
//
// The directory must live under the app's own data directory: the asset
// protocol scope in tauri.conf.json is compile-time config ($APPDATA/covers,
// $APPDATA/reader, $APPDATA/parallel/**/covers, $APPDATA/parallel/**/reader),
// so covers/reader images only display for data directories inside it. The
// dev directory therefore sits at `<app data dir>/parallel/dev-data` — the
// same `parallel/` namespace the UI verification lanes use (DR-162), covered
// by the existing scope. An explicitly set BOOKKEEPER_DATA_DIR still wins;
// if it points outside the app data directory the launcher says so, because
// cover thumbnails will not display there.
// Usage (via `pnpm run tauri:dev` from ui/): node scripts/dev-app.mjs [extra tauri dev args…]
import { mkdirSync, readFileSync } from "node:fs";
import { spawn } from "node:child_process";
import { homedir } from "node:os";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cli = join(root, "ui", "node_modules", "@tauri-apps", "cli", "tauri.js");

// The identifier comes from the app config (the one source that stays true
// across identifier changes). Its data directory follows the Tauri v2
// resolution: %APPDATA% on Windows, ~/Library/Application Support on macOS,
// $XDG_DATA_HOME (default ~/.local/share) on Linux.
const identifier = JSON.parse(
  readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"),
).identifier;
const appDataDir =
  process.platform === "win32"
    ? join(process.env.APPDATA, identifier)
    : process.platform === "darwin"
      ? join(homedir(), "Library", "Application Support", identifier)
      : join(process.env.XDG_DATA_HOME || join(homedir(), ".local", "share"), identifier);

const isUnder = (path, base) => {
  const rel = relative(base, resolve(path));
  return rel !== "" && !rel.startsWith("..") && !isAbsolute(rel);
};

const env = { ...process.env };
if (!env.BOOKKEEPER_DATA_DIR?.trim()) {
  env.BOOKKEEPER_DATA_DIR = join(appDataDir, "parallel", "dev-data");
} else if (!isUnder(env.BOOKKEEPER_DATA_DIR, appDataDir)) {
  console.warn(
    `[dev-app] BOOKKEEPER_DATA_DIR is outside the app data directory (${appDataDir}); ` +
      "the asset protocol only serves covers/reader files under it, so thumbnails will not display.",
  );
}
mkdirSync(env.BOOKKEEPER_DATA_DIR, { recursive: true });
console.log(`[dev-app] data directory: ${env.BOOKKEEPER_DATA_DIR}`);

const child = spawn(process.execPath, [cli, "dev", ...process.argv.slice(2)], {
  cwd: root,
  env,
  stdio: "inherit",
});
child.on("exit", (code, signal) => process.exit(signal ? 1 : (code ?? 1)));
