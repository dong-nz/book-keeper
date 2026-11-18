#!/usr/bin/env node
// Fetches the pre-built Pdfium dynamic library (DR-50c) into
// src-tauri/binaries/ so the cover renderer can bind it at runtime.
// Usage: node scripts/fetch-pdfium.mjs [--force] [--universal]
//   --universal (darwin only, DR-359): fetch both pre-built macOS assets
//   from the same release and join them with `lipo -create` — the sidecar
//   the universal macOS bundle (DR-358 a) carries for both architectures.
// Prerequisite for building/running the app; documented in README.
import { mkdirSync, existsSync, readFileSync, copyFileSync, rmSync, readdirSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const targetDir = join(root, "src-tauri", "binaries");

const platforms = {
  "win32:x64": { asset: "pdfium-win-x64.tgz", lib: "pdfium.dll" },
  "win32:arm64": { asset: "pdfium-win-arm64.tgz", lib: "pdfium.dll" },
  "linux:x64": { asset: "pdfium-linux-x64.tgz", lib: "libpdfium.so" },
  "linux:arm64": { asset: "pdfium-linux-arm64.tgz", lib: "libpdfium.so" },
  "darwin:x64": { asset: "pdfium-mac-x64.tgz", lib: "libpdfium.dylib" },
  "darwin:arm64": { asset: "pdfium-mac-arm64.tgz", lib: "libpdfium.dylib" },
};

const universal = process.argv.includes("--universal");
if (universal && process.platform !== "darwin") {
  console.error("--universal is darwin-only (lipo is a macOS tool)");
  process.exit(1);
}

const platform = universal
  ? platforms[`darwin:${process.arch}`] // the target library name is the same
  : platforms[`${process.platform}:${process.arch}`];
if (!platform) {
  console.error(`no pre-built Pdfium mapping for ${process.platform}:${process.arch}`);
  process.exit(1);
}
const target = join(targetDir, platform.lib);

if (existsSync(target) && !process.argv.includes("--force")) {
  console.log(`${platform.lib} already present: ${target}`);
  process.exit(0);
}

const release = await (
  await fetch("https://api.github.com/repos/bblanchon/pdfium-binaries/releases/latest", {
    headers: { accept: "application/vnd.github+json" },
  })
).json();

const findFile = (dir, name) => {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      const found = findFile(path, name);
      if (found) return found;
    } else if (entry.name === name) {
      return path;
    }
  }
  return null;
};

// One asset of the release, extracted to a working directory: returns the
// path of the library file inside. Shared by the per-arch and the universal
// path — the fetch, the archive, the extraction and the search are one code
// path (no second implementation, RULES R1).
async function fetchAsset(assetName, libName, work) {
  const asset = release.assets.find((a) => a.name === assetName);
  if (!asset) {
    console.error(`${assetName} not found in release ${release.tag_name}`);
    process.exit(1);
  }
  console.log(`fetching ${asset.name} from release ${release.tag_name} …`);
  const bytes = Buffer.from(await (await fetch(asset.browser_download_url)).arrayBuffer());
  rmSync(work, { recursive: true, force: true });
  mkdirSync(work, { recursive: true });
  const archive = join(work, "pdfium.tgz");
  writeFileSync(archive, bytes);
  // cwd + relative names: an absolute Windows path carries a drive colon,
  // which Git-Bash's GNU tar reads as host:path ("Cannot connect to D:" —
  // the CI Windows run, DR-383). Both GNU tar and bsdtar extract this form.
  execFileSync("tar", ["-xzf", "pdfium.tgz"], { cwd: work });
  const found = findFile(work, libName);
  if (!found) {
    console.error(`${libName} not found inside the extracted archive`);
    process.exit(1);
  }
  return found;
}

if (universal) {
  // DR-359: both pre-built macOS libraries joined into one fat library.
  const x64 = await fetchAsset("pdfium-mac-x64.tgz", platform.lib, join(targetDir, ".fetch-tmp-x64"));
  const arm64 = await fetchAsset(
    "pdfium-mac-arm64.tgz",
    platform.lib,
    join(targetDir, ".fetch-tmp-arm64")
  );
  execFileSync("lipo", ["-create", x64, arm64, "-output", target]);
  for (const work of [join(targetDir, ".fetch-tmp-x64"), join(targetDir, ".fetch-tmp-arm64")]) {
    rmSync(work, { recursive: true, force: true });
  }
  console.log(`installed universal ${target} (${readFileSync(target).byteLength} bytes, release ${release.tag_name})`);
} else {
  const found = await fetchAsset(
    platform.asset,
    platform.lib,
    join(targetDir, ".fetch-tmp")
  );
  copyFileSync(found, target);
  rmSync(join(targetDir, ".fetch-tmp"), { recursive: true, force: true });
  console.log(`installed ${target} (${readFileSync(target).byteLength} bytes, release ${release.tag_name})`);
}
