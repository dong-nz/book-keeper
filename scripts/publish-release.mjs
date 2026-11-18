#!/usr/bin/env node
// NFR-11 (SPEC-v0.0.11 §3.4/§3.5, DR-358): assemble the `latest.json` update
// manifest from `tauri build`'s outputs. Pure file reading: the artifacts and
// their `.sig` files are produced by the release job's build steps; the URLs
// are deterministic (`…/releases/download/<tag>/<file>`), so the manifest is
// complete before the GitHub Release exists. The plugin's manifest lookup is
// exact `{os}-{arch}` (tauri-plugin-updater `updater.rs::target()`); the
// darwin keys both resolve to the universal artifact (DR-358 a). The notes
// text is the first section of the Ship notes page — the pipeline consumes
// that page rather than duplicating it (§3.5).
//
// Usage:
//   node scripts/publish-release.mjs \
//     --dist <dir with the built bundles + .sig files> \
//     --version <x.y.z>            (must match the app version and the tag)
//     --tag <vX.Y.Z>               (the release tag the URLs point at)
//     --repo <owner>/<name>
//     --notes <docs/release/notes/v<version>.md>
//     --out <latest.json path>
//     [--pub-date <ISO 8601>]      (default: now)

import { readFileSync, readdirSync, existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";

function arg(name) {
  const i = process.argv.indexOf(`--${name}`);
  if (i === -1 || i === process.argv.length - 1) return null;
  return process.argv[i + 1];
}

const dist = arg("dist");
const version = arg("version");
const tag = arg("tag");
const repo = arg("repo");
const notesPath = arg("notes");
const out = arg("out");
const pubDate = arg("pub-date") ?? new Date().toISOString();
for (const [name, value] of Object.entries({ dist, version, tag, repo, notes: notesPath, out })) {
  if (!value) {
    console.error(`missing --${name}`);
    process.exit(2);
  }
}

// The tag carries the version; a pre-flight tag may add a `-suffix`
// (release.yml publishes those as drafts — NFR-11).
const versionEsc = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
if (!new RegExp(`^v${versionEsc}(-[0-9A-Za-z.-]+)?$`).test(tag)) {
  console.error(`tag ${tag} does not match version ${version}`);
  process.exit(2);
}

// The notes page's first section: the body under the first `## ` heading up
// to the next one (or EOF) — the heading itself is page furniture, not
// release notes. The page is the plain-language summary; the manifest carries
// its body, which is what the in-app *Release notes* shows (UI-52).
function notesText() {
  const page = readFileSync(notesPath, "utf8");
  const start = page.search(/^## /m);
  if (start === -1) {
    console.error(`no section heading in ${notesPath}`);
    process.exit(2);
  }
  const body = page.slice(page.indexOf("\n", start) + 1);
  const next = body.search(/^## /m);
  return (next === -1 ? body : body.slice(0, next)).trim();
}

// Updater artifacts per manifest key (DR-358 a): the NSIS setup exe (Windows),
// the universal app tarball (both darwin keys), the AppImage (Linux). Every
// one must carry the version in its file name — `tauri build` names them
// `<Product>_<version>_<arch>…` — and its `.sig` sibling.
const EXPECTED = [
  { key: "windows-x86_64", match: (f) => /_x64-setup\.exe$/.test(f) },
  { key: "darwin-aarch64", match: (f) => /_universal\.app\.tar\.gz$/.test(f) },
  { key: "darwin-x86_64", match: (f) => /_universal\.app\.tar\.gz$/.test(f) },
  { key: "linux-x86_64", match: (f) => /_x86_64\.AppImage$/.test(f) },
];

const files = readdirSync(dist);
const platforms = {};
const problems = [];
for (const { key, match } of EXPECTED) {
  const file = files.find(match);
  if (!file) {
    problems.push(`no artifact for ${key}`);
    continue;
  }
  if (!file.includes(`_${version}_`)) {
    problems.push(`${file} does not carry version ${version}`);
    continue;
  }
  const sigPath = join(dist, `${file}.sig`);
  if (!existsSync(sigPath)) {
    problems.push(`${file} has no .sig sibling`);
    continue;
  }
  // The manifest `signature` field is the .sig file's base64 text verbatim —
  // the form the plugin's `base64_to_string` step expects (DR-329's probe).
  platforms[key] = {
    url: `https://github.com/${repo}/releases/download/${tag}/${file}`,
    signature: readFileSync(sigPath, "utf8").trim(),
  };
}
if (problems.length) {
  console.error(problems.join("\n"));
  process.exit(1);
}

const manifest = {
  version,
  notes: notesText(),
  pub_date: pubDate,
  platforms,
};
writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(
  `latest.json written to ${out}: version ${version}, ` +
    `${Object.keys(platforms).length} platform entries`
);
