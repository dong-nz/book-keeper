#!/usr/bin/env node
// Generates the test fixtures (RULES R6.2): deterministic, never
// committed as binaries. Rust tests and the UI smoke script consume the
// same tree, so fixture semantics are defined in exactly one place.
// Usage: node scripts/make-fixtures.mjs
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { deflateRawSync, deflateSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import crypto from "node:crypto";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const fixtures = join(root, "fixtures");

const DOC_COUNT = 10_000;

// AC-131 (SPEC-v0.0.9 §3.8): the mixed 10 000-document corpus. Every
// SCALE_MIX_STEP-th index from SCALE_MIX_FROM on is an EPUB carrying the
// same title/author shape as the PDFs it replaces — 1 000 EPUBs inside the
// 10 000-document corpus the 10k runner imports.
const SCALE_MIX_FROM = 5005;
const SCALE_MIX_STEP = 5;

// Core fixture tier (SPEC-v0.0.5 §2, DR-135): the 100 documents the functional
// smoke flows run against. The composition carries exactly the properties those
// flows depend on: identical byte sizes (the sort-tie property), Info
// dictionaries with Title/Author (metadata display, search), one document with
// no Info (doc00900 — the filename fallback), one document the missing-file
// flow moves away (doc00500), and the title prefixes the search and tag
// flows count on (doc00100..102, doc00200..202, doc00300..301). The manifest
// records the composition so the harness derives its expected counts from it
// instead of hard-coding them.
const CORE_DOC_COUNT = 100;
const coreNames = [];
for (let index = 1; index <= 90; index += 1) coreNames.push(`doc${String(index).padStart(5, "0")}`);
for (const [from, to] of [[100, 102], [200, 202], [300, 301]]) {
  for (let index = from; index <= to; index += 1) coreNames.push(`doc${String(index).padStart(5, "0")}`);
}
coreNames.push("doc00500"); // the missing-file flow's document
coreNames.push("doc00900"); // the no-Info document (filename fallback)

// Minimal valid PDF assembled with real xref offsets. `info` adds an Info
// dictionary (Title/Author as literal strings); omitting it yields a valid
// PDF with no metadata (filename-fallback fixture).
function buildPdf({ title, author }) {
  const stream = "BT /F1 14 Tf 72 720 Td (fixture) Tj ET";
  const objects = [
    [1, "<< /Type /Catalog /Pages 2 0 R >>"],
    [2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"],
    [
      3,
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R " +
        "/Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>",
    ],
    [4, `<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`],
  ];
  const trailerParts = [`/Size ${objects.length + 2}`, "/Root 1 0 R"];
  if (title !== undefined || author !== undefined) {
    const infoFields = [];
    if (title !== undefined) infoFields.push(`/Title ${literal(title)}`);
    if (author !== undefined) infoFields.push(`/Author ${literal(author)}`);
    objects.push([5, `<< ${infoFields.join(" ")} >>`]);
    trailerParts.push("/Info 5 0 R");
  }
  // Fixed ID keeps the file byte-deterministic (R6.2).
  trailerParts.push("/ID [<00000000000000000000000000000000> <00000000000000000000000000000000>]");
  return assemble(objects, `<< ${trailerParts.join(" ")} >>`);
}

// Multi-page fixture documents (SPEC-v0.0.6 §3.5): deterministic per-page
// content — the page number drawn in each page's own content stream — so a
// rendered page is distinguishable and reader flows can assert page identity
// (AC-61/AC-65). Same Letter MediaBox as every other fixture (612 × 792 pt).
// SPEC-v0.0.7 §3.5: an optional outline builder — catalog /Outlines +
// /PageMode /UseOutlines, item dictionaries with /Title, /Parent, /First,
// /Last, /Prev, /Next and /Dest [<page ref> /XYZ null null null]. `outline`
// is a flat [{ title, page, depth }] list; depth 1 is top level.
function buildMultiPagePdf({ pages, title, outline }) {
  const objects = [
    [1, "<< /Type /Catalog /Pages 2 0 R >>"],
    [
      2,
      `<< /Type /Pages /Kids [${Array.from(
        { length: pages },
        (_, index) => `${3 + 2 * index} 0 R`,
      ).join(" ")}] /Count ${pages} >>`,
    ],
  ];
  for (let page = 1; page <= pages; page += 1) {
    const pageObject = 3 + 2 * (page - 1);
    const contentObject = pageObject + 1;
    objects.push([
      pageObject,
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents ${contentObject} 0 R ` +
        "/Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>",
    ]);
    const stream = `BT /F1 24 Tf 72 700 Td (Page ${page} of ${pages}) Tj ET`;
    objects.push([contentObject, `<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`]);
  }
  const pageObjectOf = (page) => 3 + 2 * (page - 1);
  let catalogExtras = "";
  if (outline !== undefined && outline.length > 0) {
    // Object numbering continues after the content objects; the Info
    // dictionary is reserved last so every document keeps the same trailer
    // shape.
    const outlinesRoot = objects.length + 1;
    const itemIds = new Map(); // index in the flat list -> object number
    for (let index = 0; index < outline.length; index += 1) {
      itemIds.set(index, outlinesRoot + 1 + index);
    }
    // Sibling runs share a parent; children are the maximal run of deeper
    // entries directly after their parent.
    const childrenOf = (index) => {
        const depth = outline[index].depth;
        const kids = [];
        for (let next = index + 1; next < outline.length && outline[next].depth > depth; next += 1) {
            if (outline[next].depth === depth + 1) kids.push(next);
        }
        return kids;
    };
    const siblingsOf = (index) => {
        const depth = outline[index].depth;
        const parent = parentOf(index);
        return outline
            .map((entry, entryIndex) => ({ entry, entryIndex }))
            .filter(({ entry, entryIndex }) =>
                entry.depth === depth && entryIndex !== index && parentOf(entryIndex) === parent)
            .map(({ entryIndex }) => entryIndex);
    };
    const parentOf = (index) => {
        const depth = outline[index].depth;
        if (depth === 1) return outlinesRoot;
        for (let earlier = index - 1; earlier >= 0; earlier -= 1) {
            if (outline[earlier].depth === depth - 1) return itemIds.get(earlier);
        }
        throw new Error("outline entry without a parent");
    };
    const topLevel = outline.map((entry, index) => ({ entry, index })).filter(({ entry }) => entry.depth === 1).map(({ index }) => index);
    for (let index = 0; index < outline.length; index += 1) {
      const { title: entryTitle, page } = outline[index];
      const id = itemIds.get(index);
      const kids = childrenOf(index);
      const prev = siblingsOf(index).filter((sibling) => sibling < index).pop();
      const next = siblingsOf(index).find((sibling) => sibling > index);
      const parts = [
        `/Title ${literal(entryTitle)}`,
        `/Parent ${parentOf(index)} 0 R`,
        `/Dest [${pageObjectOf(page)} 0 R /XYZ null null null]`,
      ];
      if (prev !== undefined) parts.push(`/Prev ${itemIds.get(prev)} 0 R`);
      if (next !== undefined) parts.push(`/Next ${itemIds.get(next)} 0 R`);
      if (kids.length > 0) {
        parts.push(`/First ${itemIds.get(kids[0])} 0 R`);
        parts.push(`/Last ${itemIds.get(kids[kids.length - 1])} 0 R`);
        parts.push(`/Count ${kids.length}`);
      }
      objects.push([id, `<< ${parts.join(" ")} >>`]);
    }
    const firstTop = topLevel[0];
    const lastTop = topLevel[topLevel.length - 1];
    objects.splice(objects.length, 0, [
      outlinesRoot,
      `<< /First ${itemIds.get(firstTop)} 0 R /Last ${itemIds.get(lastTop)} 0 R /Count ${topLevel.length} >>`,
    ]);
    // The outlines root must be referenced before Info is appended.
    catalogExtras = ` /Outlines ${outlinesRoot} 0 R /PageMode /UseOutlines`;
  }
  objects[0] = [1, `<< /Type /Catalog /Pages 2 0 R${catalogExtras} >>`];
  const infoObject = objects.length + 1;
  objects.push([infoObject, `<< /Title ${literal(title)} /Author ${literal("Fixture Author")} >>`]);
  const trailer =
    `<< /Size ${infoObject + 1} /Root 1 0 R /Info ${infoObject} 0 R ` +
    "/ID [<00000000000000000000000000000000> <00000000000000000000000000000000>] >>";
  return assemble(objects, trailer);
}

// A structurally valid PDF whose Info Title is non-UTF-8 ciphertext and
// whose trailer carries /Encrypt — the encrypted fallback fixture (FR-05):
// strict UTF-8 must reject it and the caller falls back to the filename.
function buildEncryptedPdf() {
  const objects = [
    [1, "<< /Type /Catalog /Pages 2 0 R >>"],
    [2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"],
    [3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>"],
    [4, "<< /Filter /Standard /V 1 /R 3 /Length 40 " +
      "/O <28bfa9f4b8e9c9f4b8e9c9f4b8e9c9f4> /U <00112233445566778899aabbccddeeff> >>"],
    [5, "<< /Title <c3a9c3b8fffe0080c3a9c3b8> /Author <fffe0100ffff> >>"],
  ];
  const trailer =
    "<< /Size 6 /Root 1 0 R /Info 5 0 R /Encrypt 4 0 R " +
    "/ID [<00000000000000000000000000000000> <00000000000000000000000000000000>] >>";
  return assemble(objects, trailer);
}

// A genuinely encrypted PDF (FR-20, AC-104…AC-106): Standard security
// handler, V1/R2 — 40-bit RC4, Revision 2 key derivation (no /ID salt, no
// MD5 iteration) — the oldest handler pdfium accepts. The user password is
// "sesame" (the owner password differs); without it pdfium refuses the file
// with a password error, with it every page renders. The Rust and UI tests
// name the same password; it is part of the fixture.
const PROTECTED_PASSWORD = "sesame";
const PROTECTED_OWNER_PASSWORD = "sesame-owner";

// pdfium's own pad string (`kDefaultPasscode` in cpdf_security_handler.cpp
// — it deviates from the spec's Table 21 padding and is what the app's
// renderer actually uses): the fixture's keys are computed with it so the
// app's own pdfium accepts the password.
const PDF_PASSWORD_PAD = Buffer.from([
  0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01,
  0x08, 0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53,
  0x69, 0x7a,
]);

function rc4(key, data) {
  const s = Array.from({ length: 256 }, (_, i) => i);
  for (let i = 0, j = 0; i < 256; i += 1) {
    j = (j + s[i] + key[i % key.length]) % 256;
    [s[i], s[j]] = [s[j], s[i]];
  }
  const out = Buffer.alloc(data.length);
  for (let k = 0, i = 0, j = 0; k < data.length; k += 1) {
    i = (i + 1) % 256;
    j = (j + s[i]) % 256;
    [s[i], s[j]] = [s[j], s[i]];
    out[k] = data[k] ^ s[(s[i] + s[j]) % 256];
  }
  return out;
}

function padPassword(password) {
  const bytes = Buffer.from(password, "latin1");
  if (bytes.length >= 32) return bytes.subarray(0, 32);
  return Buffer.concat([bytes, PDF_PASSWORD_PAD.subarray(0, 32 - bytes.length)]);
}

const md5 = (...parts) => crypto.createHash("md5").update(Buffer.concat(parts)).digest();

function buildProtectedPdf() {
  const ownerPad = padPassword(PROTECTED_OWNER_PASSWORD);
  const userPad = padPassword(PROTECTED_PASSWORD);
  // Algorithm 3 (Rev 2): the /O entry.
  const o = rc4(md5(ownerPad).subarray(0, 5), userPad);
  // Permissions: all bits set (the fixture is a fixture).
  const perms = Buffer.from([0xff, 0xff, 0xff, 0xff]);
  // Algorithm 2 (Rev 2) as pdfium implements it: the file key hashes the
  // padded password, /O, the permissions and the /ID bytes (pdfium mixes
  // the ID for every revision — a documented deviation from the spec).
  const fileId = Buffer.from("00000000000000000000000000000000", "hex");
  const fileKey = md5(userPad, o, perms, fileId).subarray(0, 5);
  const u = rc4(fileKey, PDF_PASSWORD_PAD);
  // Algorithm 4 (Rev 2): the per-object key for object 4, generation 0 —
  // the hash is file key + object number (low 3) + generation (low 2), and
  // the key is file-key-length + 5 bytes (10 here; pdfium uses the same).
  const objectKey = md5(
    fileKey,
    Buffer.from([4 & 0xff, (4 >> 8) & 0xff, (4 >> 16) & 0xff, 0, 0]),
  ).subarray(0, 10);
  const plain = Buffer.from(
    "BT /F1 24 Tf 72 700 Td (Protected fixture page) Tj ET\n",
    "latin1",
  );
  const cipher = rc4(objectKey, plain);
  const objects = [
    [1, "<< /Type /Catalog /Pages 2 0 R >>"],
    [2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"],
    [
      3,
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] " +
        "/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    ],
    // The stream body is binary RC4 output; latin1 round-trips every byte.
    [
      4,
      `<< /Length ${cipher.length} >>\nstream\n${cipher.toString("latin1")}\nendstream`,
    ],
    [5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"],
    [
      6,
      `<< /Filter /Standard /V 1 /R 2 /Length 40 /P -1 ` +
        `/O <${o.toString("hex")}> /U <${u.toString("hex")}> >>`,
    ],
  ];
  const trailer =
    "<< /Size 7 /Root 1 0 R /Encrypt 6 0 R " +
    `/ID [<${fileId.toString("hex")}> <${fileId.toString("hex")}>] >>`;
  return assemble(objects, trailer);
}

function assemble(objects, trailerBody) {
  let bytes = Buffer.from("%PDF-1.4\n", "latin1");
  const offsets = new Map();
  for (const [num, body] of objects) {
    offsets.set(num, bytes.length);
    bytes = Buffer.concat([bytes, Buffer.from(`${num} 0 obj\n${body}\nendobj\n`, "latin1")]);
  }
  const highest = Math.max(...objects.map(([num]) => num));
  const xrefStart = bytes.length;
  let xref = `xref\n0 ${highest + 1}\n0000000000 65535 f \n`;
  for (let num = 1; num <= highest; num += 1) {
    xref += `${String(offsets.get(num)).padStart(10, "0")} 00000 n \n`;
  }
  return Buffer.concat([
    bytes,
    Buffer.from(`${xref}trailer\n${trailerBody}\nstartxref\n${xrefStart}\n%%EOF\n`, "latin1"),
  ]);
}

function literal(text) {
  const escaped = text.replace(/[\\()]/g, (c) => `\\${c}`);
  return `(${escaped})`;
}

// Pads a fixture to an exact byte size with a trailing comment line. The xref
// is located by startxref offset, so bytes after %%EOF are ignored by the
// reader; this exists so the no-Info core document carries the same byte size
// as the Info documents (the core tier's sort-tie property).
function padToSize(bytes, target) {
  const pad = target - bytes.length;
  if (pad <= 0) return bytes;
  return Buffer.concat([bytes, Buffer.from(`% ${"p".repeat(pad - 3)}\n`, "latin1")]);
}

rmSync(fixtures, { recursive: true, force: true });
mkdirSync(join(fixtures, "docs"), { recursive: true });
mkdirSync(join(fixtures, "special"), { recursive: true });
mkdirSync(join(fixtures, "core"), { recursive: true });

for (let index = 1; index <= DOC_COUNT; index += 1) {
  // AC-131 (SPEC-v0.0.9 §3.8): every fifth document in the 5001..10000 band
  // is an EPUB instead — the mixed corpus the 10k runner imports. The band
  // avoids every name the harness flows reference individually (doc01001…
  // 01042), so those flows keep their exact PDF paths. The EPUBs are written
  // below, where the EPUB writer is defined.
  if (index >= SCALE_MIX_FROM && index % SCALE_MIX_STEP === 0) continue;
  const name = `doc${String(index).padStart(5, "0")}`;
  const pdf = buildPdf({ title: `Fixture Doc ${name}`, author: "Fixture Author" });
  writeFileSync(join(fixtures, "docs", `${name}.pdf`), pdf);
}

// Core tier: every document the same byte size (the no-Info one is padded to
// the Info size), titles in the same "Fixture Doc <name>" shape the docs tier
// uses, and a manifest.json the harness reads to derive its expected counts.
const coreInfoSize = buildPdf({ title: "Fixture Doc doc00001", author: "Fixture Author" }).length;
const coreManifest = { docs: [] };
for (const name of coreNames) {
  const noInfo = name === "doc00900";
  const pdf = noInfo
    ? padToSize(buildPdf({}), coreInfoSize)
    : buildPdf({ title: `Fixture Doc ${name}`, author: "Fixture Author" });
  writeFileSync(join(fixtures, "core", `${name}.pdf`), pdf);
  // UI-36 (AC-85): the manifest records each document's byte size so the
  // row meta-column flow asserts the rendered size cell against it.
  coreManifest.docs.push(noInfo
    ? { name: `${name}.pdf`, noInfo: true, size: pdf.length }
    : { name: `${name}.pdf`, title: `Fixture Doc ${name}`, author: "Fixture Author", size: pdf.length });
}
writeFileSync(join(fixtures, "core", "manifest.json"), `${JSON.stringify(coreManifest, null, 2)}\n`);

// Multi-page tier (SPEC-v0.0.6 §3.5): five core-tier documents (12 / 24 / 48 /
// 96 / 192 pages) and one scale-tier 1 000-page document, generated into
// fixtures/multi/ — the existing single-page corpora stay byte- and
// count-identical ("covers keep their corpus"). The manifest carries the page
// counts so the harness and the Rust tests derive them instead of hard-coding.
// SPEC-v0.0.7 §3.5: multi-192 carries a fixed four-entry outline (nested
// third entry), recorded in the manifest as `outline: [{ title, page, depth }]`
// — the Rust test and the UI flow derive the expected tree from it. The other
// documents keep no outline (UI-33's negative case).
const multiTier = [
  { name: "multi-012", pages: 12, tier: "core" },
  { name: "multi-024", pages: 24, tier: "core" },
  { name: "multi-048", pages: 48, tier: "core" },
  { name: "multi-096", pages: 96, tier: "core" },
  {
    name: "multi-192",
    pages: 192,
    tier: "core",
    outline: [
      { title: "Section One", page: 1, depth: 1 },
      { title: "Section Two", page: 41, depth: 1 },
      { title: "Section Two A", page: 61, depth: 2 },
      { title: "Section Three", page: 121, depth: 1 },
    ],
  },
  { name: "multi-1000", pages: 1000, tier: "scale" },
];
mkdirSync(join(fixtures, "multi"), { recursive: true });
const multiManifest = { docs: [] };
for (const { name, pages, tier, outline } of multiTier) {
  const pdf = buildMultiPagePdf({ pages, title: `Fixture Doc ${name}`, outline });
  writeFileSync(join(fixtures, "multi", `${name}.pdf`), pdf);
  multiManifest.docs.push({
    name: `${name}.pdf`,
    pages,
    tier,
    title: `Fixture Doc ${name}`,
    author: "Fixture Author",
    ...(outline === undefined ? {} : { outline }),
  });
}
writeFileSync(join(fixtures, "multi", "manifest.json"), `${JSON.stringify(multiManifest, null, 2)}\n`);

const special = {
  // Info-dictionary hit (FR-05 primary path).
  "info-title.pdf": buildPdf({ title: "Real Info Title", author: "Real Info Author" }),
  // Valid PDF, no Info dictionary → filename fallback.
  "info-none.pdf": buildPdf({}),
  // Encrypted-looking Info strings → strict UTF-8 rejection → filename fallback.
  "encrypted.pdf": buildEncryptedPdf(),
  // Genuinely password-protected (FR-20): pdfium refuses it without the
  // password "sesame" and renders it with it. The reader's unlock flow and
  // the cover glyph fallback both key on this file.
  "protected.pdf": buildProtectedPdf(),
  // Not a PDF at all → extraction fails → filename fallback; pdfium cannot
  // render it either, so it doubles as the glyph-fallback fixture (FR-16).
  "malformed.pdf": Buffer.from("%PDF-1.7\nnot a pdf body at all\n%%EOF\n", "latin1"),
  // A non-PDF file the import pipeline must skip (FR-01).
  "notes.txt": Buffer.from("plain text, never imported\n", "latin1"),
  // The meta-column flow's distinctly-named fixture: no Info, so its title is
  // the filename fallback — a name no other fixture can produce. The spaces
  // keep it ordinary query tokens for the search flows.
  "meta column probe.pdf": buildPdf({}),
  // AC-93 (UI-30/DR-206): the long-name fixture — exactly 72 characters,
  // which at the strip's fonts is wider than the space the reader chrome has
  // for the name at the window's minimum size, while the default 1000 px
  // window still fits it in full. No Info (the name is the identity); no
  // query token collides with any flow's search terms.
  "ac93-reader-chrome-name-ellipsis-fixture-document-for-narrow-windows.pdf": buildPdf({}),
};
for (const [name, bytes] of Object.entries(special)) {
  writeFileSync(join(fixtures, "special", name), bytes);
}

// ---------------------------------------------------------------------------
// EPUB tier (SPEC-v0.0.9 §3.8): the fixtures the EPUB tests consume.
// Node has no zip writer, so one is hand-rolled here: STORED entries
// (CRC32 + local/central headers) plus DEFLATE entries via
// zlib.deflateRawSync with method-8 headers. In every archive `mimetype` is
// the first entry, STORED and uncompressed with no extra field — the EPUB
// Open Container rule the reader relies on (§3.2 read order). Deterministic
// by construction: fixed DOS timestamp, fixed content, fixed order. The same
// writer scales the mixed 10 000-document corpus (AC-131).
// ---------------------------------------------------------------------------

const CRC32_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (const byte of buf) c = CRC32_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

// entries: [{ name, bytes, deflate }] in archive order. Fixed DOS time
// (2026-11-11 00:00:00) so the archives are byte-identical across runs.
function buildZip(entries) {
  const dosTime = 0;
  const dosDate = ((2026 - 1980) << 9) | (11 << 5) | 11;
  const locals = [];
  const central = [];
  let offset = 0;
  for (const entry of entries) {
    const name = Buffer.from(entry.name, "utf8");
    const method = entry.deflate ? 8 : 0;
    const compressed = entry.deflate ? deflateRawSync(entry.bytes, { level: 6 }) : entry.bytes;
    const crc = crc32(entry.bytes);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4); // version needed
    local.writeUInt16LE(0, 6); // flags: no data descriptor
    local.writeUInt16LE(method, 8);
    local.writeUInt16LE(dosTime, 10);
    local.writeUInt16LE(dosDate, 12);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(compressed.length, 18);
    local.writeUInt32LE(entry.bytes.length, 22);
    local.writeUInt16LE(name.length, 26);
    local.writeUInt16LE(0, 28); // no extra field
    locals.push(local, name, compressed);
    const head = Buffer.alloc(46);
    head.writeUInt32LE(0x02014b50, 0);
    head.writeUInt16LE(20, 4); // version made by
    head.writeUInt16LE(20, 6); // version needed
    head.writeUInt16LE(0, 8);
    head.writeUInt16LE(method, 10);
    head.writeUInt16LE(dosTime, 12);
    head.writeUInt16LE(dosDate, 14);
    head.writeUInt32LE(crc, 16);
    head.writeUInt32LE(compressed.length, 20);
    head.writeUInt32LE(entry.bytes.length, 24);
    head.writeUInt16LE(name.length, 28);
    // 30..41: zero (comment, disk, internal/external attrs); offset at 42.
    head.writeUInt32LE(offset, 42);
    central.push(head, name);
    offset += 30 + name.length + compressed.length;
  }
  const centralBuf = Buffer.concat(central);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(entries.length, 8);
  eocd.writeUInt16LE(entries.length, 10);
  eocd.writeUInt32LE(centralBuf.length, 12);
  eocd.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, centralBuf, eocd]);
}

// Minimal 8-bit RGB PNG (filter-0 scanlines, zlib-wrapped IDAT) — the
// declared-cover fixture's raster image, decodable by the `image` crate the
// cover worker uses (§3.3).
function buildPng(width, height) {
  const raw = Buffer.alloc(height * (1 + width * 3));
  for (let y = 0; y < height; y += 1) {
    const row = y * (1 + width * 3);
    raw[row] = 0;
    for (let x = 0; x < width; x += 1) {
      const px = row + 1 + x * 3;
      raw[px] = Math.round((x * 255) / (width - 1));
      raw[px + 1] = Math.round((y * 255) / (height - 1));
      raw[px + 2] = 128;
    }
  }
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length, 0);
    const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const crcBuf = Buffer.alloc(4);
    crcBuf.writeUInt32BE(crc32(body), 0);
    return Buffer.concat([len, body, crcBuf]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // color type: truecolor RGB
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const XML_DECL = '<?xml version="1.0" encoding="UTF-8"?>\n';

const CONTAINER_XML = `${XML_DECL}<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>
`;

// The OPF: metadata (title/author when given), the manifest (nav item when
// given, cover item when given, chapter items), and the spine. `items` is
// [{ id, href, mediaType }]; `spine` is the idref list. `metaCover` is the
// cover item id for the <meta name="cover"> declaration.
function opfXml({ title, author, items, spine, navId, metaCover, tocId }) {
  const meta = [
    `<dc:identifier id="pub-id">urn:uuid:pdfkeeper-fixture</dc:identifier>`,
    ...(title === undefined ? [] : [`<dc:title>${title}</dc:title>`]),
    ...(author === undefined ? [] : [`<dc:creator>${author}</dc:creator>`]),
    '<meta property="dcterms:modified">2026-11-11T00:00:00Z</meta>',
    ...(metaCover === undefined ? [] : [`<meta name="cover" content="${metaCover}"/>`]),
  ].join("\n    ");
  const manifest = items
    .map(
      (item) =>
        `<item id="${item.id}" href="${item.href}" media-type="${item.mediaType}"${
          item.id === navId ? ' properties="nav"' : ""
        }/>`,
    )
    .join("\n    ");
  const spineBody = spine.map((id) => `<itemref idref="${id}"/>`).join("\n    ");
  return `${XML_DECL}<package xmlns="http://www.idref.org/2005/OPF" xmlns:dc="http://purl.org/dc/elements/1.1/" version="3.0" unique-identifier="pub-id" xml:lang="en">
  <metadata>
    ${meta}
  </metadata>
  <manifest>
    ${manifest}
  </manifest>
  <spine${tocId ? ` toc="${tocId}"` : ""}>
    ${spineBody}
  </spine>
</package>
`;
}

function xhtmlDoc(title, body) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en" lang="en">
<head><title>${title}</title></head>
<body>
${body}
</body>
</html>
`;
}

function chapterBody(index) {
  return `<h1>Chapter ${index}</h1>
<p>Fixture EPUB chapter ${index} body text. The quick brown fox page ${index}.</p>
<p>Second paragraph of chapter ${index} — deterministic filler for position and scroll assertions.</p>`;
}

// Assembles one EPUB archive. `stored` picks the all-STORED variant (§3.8:
// the Rust zip reader exercises both paths). `nav` adds the EPUB3 nav item;
// `cover` adds the declared cover item. `hostile` adds the AC-137 corpus
// chapter + its stylesheet. `chapters` may be a count or an explicit
// [{ index, body }] list (the stress and hostile fixtures).
function buildEpub({ title, author, chapters, nav = false, ncx = false, cover = null, stored = false, hostile = false, hrefOverrides = {}, extra = [] }) {
  const count = typeof chapters === "number" ? chapters : chapters.length;
  const items = [];
  const spine = [];
  const entries = [
    { name: "mimetype", bytes: Buffer.from("application/epub+zip", "ascii"), deflate: false },
    { name: "META-INF/container.xml", bytes: Buffer.from(CONTAINER_XML, "utf8"), deflate: !stored },
  ];
  if (cover !== null) {
    items.push({ id: "cover-item", href: cover.href, mediaType: cover.mediaType });
    entries.push({ name: `OEBPS/${cover.href}`, bytes: cover.bytes, deflate: !stored });
  }
  // Manifest-only assets (DR-304): referenced from chapter content or CSS,
  // never in the spine — the reading fixture's image, stylesheet and font.
  for (const file of extra) {
    items.push({ id: file.id, href: file.href, mediaType: file.mediaType });
    entries.push({ name: `OEBPS/${file.href}`, bytes: file.bytes, deflate: !stored });
  }
  const chapterIds = [];
  for (let index = 1; index <= count; index += 1) {
    const spec = typeof chapters === "number" ? { index, body: chapterBody(index) } : chapters[index - 1];
    const declaredHref = hrefOverrides[index] ?? `text/ch${String(index).padStart(2, "0")}.xhtml`;
    const entryName = `OEBPS/text/ch${String(index).padStart(2, "0")}.xhtml`;
    items.push({ id: `ch${index}`, href: declaredHref, mediaType: "application/xhtml+xml" });
    spine.push(`ch${index}`);
    chapterIds.push(`ch${index}`);
    const body = spec.body ?? chapterBody(index);
    entries.push({ name: entryName, bytes: Buffer.from(xhtmlDoc(`Chapter ${index}`, body), "utf8"), deflate: !stored });
  }
  if (hostile) {
    items.push({ id: "hostile-css", href: "text/hostile.css", mediaType: "text/css" });
    entries.push({
      name: "OEBPS/text/hostile.css",
      bytes: Buffer.from(
        `/* AC-137: author CSS reaching a remote host — the reader must make no request. */
@font-face { font-family: "RemoteHostFont"; src: url("http://remote.invalid/fixture.woff"); }
body { font-family: "RemoteHostFont", serif; }
`,
        "utf8",
      ),
      deflate: !stored,
    });
  }
  if (nav) {
    // Chapter 2 carries a nested sub-entry (DR-301 item 7: ToC depth comes
    // from ol nesting) — the nav parser's depth test on a real fixture.
    const tocItems = Array.from(
      { length: count },
      (_, i) => {
        const href = `text/ch${String(i + 1).padStart(2, "0")}.xhtml`;
        const entry = `<li><a href="${href}">Chapter ${i + 1}</a>`;
        if (i === 1) {
          return `    ${entry}<ol>\n      <li><a href="${href}#sub-a">Sub Two A</a></li>\n    </ol></li>`;
        }
        return `    ${entry}</li>`;
      },
    ).join("\n");
    items.push({ id: "nav", href: "nav.xhtml", mediaType: "application/xhtml+xml" });
    entries.push({
      name: "OEBPS/nav.xhtml",
      bytes: Buffer.from(
        xhtmlDoc(
          "Table of Contents",
          `<nav epub:type="toc" xmlns:epub="http://www.idref.org/2005/EPUB">
<ol>
${tocItems}
</ol>
</nav>`,
        ),
        "utf8",
      ),
      deflate: !stored,
    });
  }
  if (ncx) {
    // EPUB2 navMap (DR-301 item 10): nested entries, and one entry whose
    // src resolves to no spine item — the "not reading order" skip.
    items.push({ id: "ncx", href: "toc.ncx", mediaType: "application/x-epub-ncx+xml" });
    entries.push({
      name: "OEBPS/toc.ncx",
      bytes: Buffer.from(
        `<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:pdfkeeper-fixture"/></head>
  <docTitle><text>Table of Contents</text></docTitle>
  <navMap>
    <navPoint id="n1" playOrder="1"><navLabel><text>Chapter One</text></navLabel><content src="text/ch01.xhtml"/></navPoint>
    <navPoint id="n2" playOrder="2"><navLabel><text>Chapter Two</text></navLabel><content src="text/ch02.xhtml"/>
      <navPoint id="n2-1" playOrder="3"><navLabel><text>Sub Two A</text></navLabel><content src="text/ch02.xhtml#sub-a"/></navPoint>
    </navPoint>
    <navPoint id="n3" playOrder="4"><navLabel><text>Not In Spine</text></navLabel><content src="text/absent.xhtml"/></navPoint>
  </navMap>
</ncx>
`,
        "utf8",
      ),
      deflate: !stored,
    });
  }
  entries.push({
    name: "OEBPS/package.opf",
    bytes: Buffer.from(
      opfXml({
        title,
        author,
        items,
        spine,
        navId: nav ? "nav" : undefined,
        metaCover: cover === null ? undefined : "cover-item",
        tocId: ncx ? "ncx" : undefined,
      }),
      "utf8",
    ),
    deflate: !stored,
  });
  return buildZip(entries);
}

// The AC-137 hostile corpus: script in body, script inside inline SVG, an
// on*-handler attribute, a javascript: href, an iframe, a data:-URI image,
// and a stylesheet link to the remote-url CSS. A data: URI of the 1×1 PNG
// renders through the delivery mechanism (allowed set); everything executable
// must not run.
const HOSTILE_BODY = `<h1>Hostile Chapter</h1>
<link rel="stylesheet" href="hostile.css"/>
<script>document.body.appendChild(document.createElement("p")).textContent = "BODY SCRIPT RAN";</script>
<p onclick="this.textContent = 'HANDLER RAN'">attribute handler</p>
<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><script>document.body.setAttribute("data-svg-script", "ran");</script><rect width="5" height="5"/></svg>
<a href="javascript:alert(1)">javascript url</a>
<iframe src="text/ch02.xhtml" title="iframe"></iframe>
<img src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==" alt="data uri image"/>`;

// The 1 MB-chapter stress fixture (§3.12/AC-138): deterministic paragraphs
// until the chapter document crosses 1 000 000 bytes uncompressed.
function stressChapterBody() {
  const parts = ["<h1>Stress Chapter</h1>"];
  let size = parts.join("\n").length;
  let n = 0;
  while (size < 1_000_000) {
    n += 1;
    const para = `<p>Stress paragraph ${String(n).padStart(5, "0")} — ${"filler ".repeat(40)}end.</p>`;
    parts.push(para);
    size += para.length + 1;
  }
  return parts.join("\n");
}

// The reading fixture (DR-304): the AC-135 link corpus (internal relative +
// fragment-only + external anchors, heading ids) and the AC-136 asset corpus
// (referenced image, stylesheet with a color marker and an @font-face).
const readingFiller = (count, offset = 0) =>
  Array.from(
    { length: count },
    (_, i) => `<p>Reading paragraph ${String(offset + i + 1).padStart(4, "0")} — deterministic filler for the position model.</p>`,
  ).join("\n");

const READING_CH1_BODY = `<h1>Reading Chapter One</h1>
<p>Opening chapter of the reading fixture — the quick brown fox.</p>
<p><a href="ch03.xhtml#sec-32">Jump to the section in chapter three</a></p>
<p><a href="https://pdfkeeper.example.invalid/external-link-target">Open the external link</a></p>
<img src="../images/reading.png" alt="Fixture image"/>
<p class="css-marker">A paragraph the fixture stylesheet colors.</p>
<p class="font-test">A paragraph in the embedded fixture font.</p>
${readingFiller(6)}
<p id="local-note">The fragment target inside this chapter.</p>
<p><a href="#local-note">Jump to the note in this chapter</a></p>
<link rel="stylesheet" href="../styles/reading.css"/>`;

const READING_CH2_BODY = `<h1>Reading Chapter Two</h1>
<h2 id="sub-a">Section Two A — the nested ToC target</h2>
${readingFiller(30)}`;

const READING_CH3_BODY = `<h1>Reading Chapter Three</h1>
${readingFiller(5)}
<h2 id="sec-32">Section 3.2 — the fragment target</h2>
<p>The paragraph right after the fragment target.</p>
${readingFiller(5, 5)}`;

const READING_CH4_BODY = `<h1>Reading Chapter Four</h1>
${readingFiller(30)}`;

const READING_CSS = Buffer.from(
  `/* Reading fixture stylesheet (DR-304): the marker rule proves the stylesheet applied through the delivery mechanism; the @font-face proves the embedded font is requested for rendered text. */
@font-face { font-family: "FixtureReadingFont"; src: url("../fonts/reading.ttf"); }
.css-marker { color: rgb(185, 28, 28); }
.font-test { font-family: "FixtureReadingFont", serif; }
`,
  "utf8",
);

// The embedded font (DR-304): a deterministic 1 544-byte TrueType built once
// with opentype.js (.notdef + a square 'A', 1000 upem) and validated in a
// real Chromium — document.fonts.load resolves, document.fonts.check
// ("48px FixtureReadingFont","A") is true (sha256
// f5ec931606861cdaca732d4889603a3cdd5c7fe447ad0e74081335cec8cae7d1). The
// verified bytes are embedded so the generator adds no dependency.
const READING_FONT_TTF = Buffer.from(
  `T1RUTwAKAIAAAwAgQ0ZGIH16YH4AAAU4AAAAtE9TLzJpUmP/AAABEAAAAGBjbWFwAAwAdAAABOwAAAAsaGVhZC8+IkYAAACsAAAA
NmhoZWEDIgGTAAAA5AAAACRobXR4AlgAAAAABewAAAAIbHRhZ2V+AAQAAAX0AAAAEm1heHAAAlAAAAABCAAAAAZuYW1l3545SgAA
AXAAAAN7cG9zdAADAAAAAAUYAAAAIAABAAAAAQAAuQGEY18PPPUAAwPoAAAAAOcZ71UAAAAA5xnvVQAAAAAB9AK8AAAAAwACAAAA
AAAAAAEAAAMg/zgAAAJYAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAACAABQAAACAAAAAwJYAfQABQAAAooCuwAAAIwCigK7AAAB3wAx
AQIAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAWFhYWABAAEEAQQMg/zgAAAK8AAAAAAABAAAAAAGQArwAAAAAAAAAAAAzAmoA
AAAEAAAAAAACAFYAAAAEAAAAAQAkAAAAAAAEAAAAAgAOACQAAAAEAAAAAwA6AHwAAAAEAAAABAA0ADIAAAAEAAAABQAWAGYAAAAE
AAAABgAyAAAAAAAEAAAABwACAFYAAAAEAAAACAACAFYAAAAEAAAACQACAFYAAAAEAAAACgACAFYAAAAEAAAACwACAFYAAAAEAAAA
DAACAFYAAAAEAAAADQACAFYAAAAEAAAADgACAFYAAAAEAAAAEAAkAAAAAAAEAAAAEQAOACQAAQAAAAAAAAABAFcAAQAAAAAAAQAS
ALYAAQAAAAAAAgAHAMgAAQAAAAAAAwAdAPQAAQAAAAAABAAaAM8AAQAAAAAABQALAOkAAQAAAAAABgAZALYAAQAAAAAABwABAFcA
AQAAAAAACAABAFcAAQAAAAAACQABAFcAAQAAAAAACgABAFcAAQAAAAAACwABAFcAAQAAAAAADAABAFcAAQAAAAAADQABAFcAAQAA
AAAADgABAFcAAQAAAAAAEAASALYAAQAAAAAAEQAHAMgAAwABBAkAAAACAFYAAwABBAkAAQAkAAAAAwABBAkAAgAOACQAAwABBAkA
AwA6AHwAAwABBAkABAA0ADIAAwABBAkABQAWAGYAAwABBAkABgAyAAAAAwABBAkABwACAFYAAwABBAkACAACAFYAAwABBAkACQAC
AFYAAwABBAkACgACAFYAAwABBAkACwACAFYAAwABBAkADAACAFYAAwABBAkADQACAFYAAwABBAkADgACAFYAAwABBAkAEAAkAAAA
AwABBAkAEQAOACQARgBpAHgAdAB1AHIAZQBSAGUAYQBkAGkAbgBnAEYAbwBuAHQAUgBlAGcAdQBsAGEAcgBGAGkAeAB0AHUAcgBl
AFIAZQBhAGQAaQBuAGcARgBvAG4AdAAgAFIAZQBnAHUAbABhAHIAVgBlAHIAcwBpAG8AbgAgADAALgAxACAAOgAgAEYAaQB4AHQA
dQByAGUAUgBlAGEAZABpAG4AZwBGAG8AbgB0ACAAUgBlAGcAdQBsAGEAckZpeHR1cmVSZWFkaW5nRm9udFJlZ3VsYXJGaXh0dXJl
UmVhZGluZ0ZvbnQgUmVndWxhclZlcnNpb24gMC4xIDogRml4dHVyZVJlYWRpbmdGb250IFJlZ3VsYXIAAAAAAQADAAEAAAAMAAQA
IAAAAAQABAABAAAAQf//AAAAQf///8AAAQAAAAAAAwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEABAEAAQEBGkZpeHR1
cmVSZWFkaW5nRm9udFJlZ3VsYXIAAQEBJ/gbAPgcAvgdA/geBIuL+bT47AUdAAAAlw8dAAAAmhGLHQAAALQSAAUBAQwmOD9AVmVy
c2lvbiAwLjFGaXh0dXJlUmVhZGluZ0ZvbnQgUmVndWxhckZpeHR1cmVSZWFkaW5nRm9udFJlZ3VsYXJBAAAAAYsAAgEBAxWLDvjs
i4sV+IiLBYv5UAX8iIsFDgAAAAACWAAAAAAAAQAAAAAAAAABABAAAmVuAAA=`,
  "base64",
);

const COVER_PNG = buildPng(120, 160);
const COVER_SVG = Buffer.from(
  `${XML_DECL}<svg xmlns="http://www.w3.org/2000/svg" width="120" height="160"><rect width="120" height="160" fill="#444"/></svg>\n`,
  "utf8",
);

mkdirSync(join(fixtures, "epub"), { recursive: true });

const epubTier = [
  // Metadata + declared raster cover + nav: the primary-path fixture
  // (AC-139, AC-127 raster half, AC-132/135 ToC half).
  {
    name: "epub-meta.epub",
    bytes: buildEpub({
      title: "Real EPUB Title",
      author: "Real EPUB Author",
      chapters: 3,
      nav: true,
      cover: { href: "images/cover.png", mediaType: "image/png", bytes: COVER_PNG },
    }),
    manifest: { title: "Real EPUB Title", author: "Real EPUB Author", chapters: 3, nav: true, cover: { href: "images/cover.png", mediaType: "image/png" } },
  },
  // No dc:title / dc:creator → the filename fallback (AC-140).
  {
    name: "epub-no-meta.epub",
    bytes: buildEpub({ title: undefined, author: undefined, chapters: 1 }),
    manifest: { chapters: 1, noMetadata: true },
  },
  // Declared cover is SVG → glyph fallback this release (AC-127 SVG half).
  {
    name: "epub-svg-cover.epub",
    bytes: buildEpub({
      title: "SVG Cover Book",
      author: "SVG Author",
      chapters: 1,
      cover: { href: "images/cover.svg", mediaType: "image/svg+xml", bytes: COVER_SVG },
    }),
    manifest: { title: "SVG Cover Book", author: "SVG Author", chapters: 1, cover: { href: "images/cover.svg", mediaType: "image/svg+xml" } },
  },
  // Metadata but no cover declaration → glyph fallback (AC-127 missing half).
  {
    name: "epub-no-cover.epub",
    bytes: buildEpub({ title: "No Cover Book", author: "No Cover Author", chapters: 1 }),
    manifest: { title: "No Cover Book", author: "No Cover Author", chapters: 1 },
  },
  // Not a zip at all → the M17 rule: imports with the fallback, typed error
  // only when the app is asked to read it (AC-141, AC-133).
  {
    name: "epub-malformed.epub",
    bytes: Buffer.from("this is not a zip archive, it is a plain text file pretending to be an EPUB\n", "utf8"),
    manifest: { malformed: true },
  },
  // AC-137 hostile corpus + a real second chapter the iframe targets.
  {
    name: "epub-hostile.epub",
    bytes: buildEpub({
      title: "Hostile Book",
      author: "Hostile Author",
      chapters: [
        { index: 1, body: HOSTILE_BODY },
        { index: 2, body: chapterBody(2) },
      ],
      nav: true,
      hostile: true,
    }),
    manifest: { title: "Hostile Book", author: "Hostile Author", chapters: 2, nav: true, hostile: true },
  },
  // All-STORED archive: the zip reader's stored path (§3.8).
  {
    name: "epub-stored.epub",
    bytes: buildEpub({ title: "Stored Book", author: "Stored Author", chapters: 2, stored: true }),
    manifest: { title: "Stored Book", author: "Stored Author", chapters: 2, stored: true },
  },
  // Deflated archive carrying the §3.2 href-resolution edge cases: a
  // percent-encoded href (ch%301 → ch01, the encoded digit of the real entry
  // name) and a backslash separator — the real-world forms the normalization
  // rule exists for. Entries themselves are deflated: the zip reader's
  // deflated path.
  {
    name: "epub-deflated.epub",
    bytes: buildEpub({
      title: "Deflated Book",
      author: "Deflated Author",
      chapters: 2,
      hrefOverrides: { 1: "text/ch%301.xhtml", 2: "text\\ch02.xhtml" },
    }),
    manifest: {
      title: "Deflated Book",
      author: "Deflated Author",
      chapters: 2,
      hrefs: { 1: "text/ch%301.xhtml", 2: "text\\ch02.xhtml" },
    },
  },
  // EPUB2 navMap book (DR-301 item 10): the middle branch of the binding ToC
  // source order (nav → NCX → spine-derived) — nested entries and one entry
  // whose src resolves outside the spine.
  {
    name: "epub-ncx.epub",
    bytes: buildEpub({ title: "Ncx Book", author: "Ncx Author", chapters: 3, ncx: true }),
    manifest: { title: "Ncx Book", author: "Ncx Author", chapters: 3, ncx: true },
  },
  // The reader-lane reading fixture (DR-304): internal relative + fragment-only
  // + external links, heading ids, a referenced image, a stylesheet with a
  // color marker and an @font-face to an embedded TTF — the AC-135/AC-136
  // corpus. The nav (default shape) targets every chapter and nests
  // "Sub Two A" at ch02#sub-a, the fixture's own heading id.
  {
    name: "epub-reading.epub",
    bytes: buildEpub({
      title: "Reading Fixture Book",
      author: "Reading Fixture Author",
      chapters: [
        { index: 1, body: READING_CH1_BODY },
        { index: 2, body: READING_CH2_BODY },
        { index: 3, body: READING_CH3_BODY },
        { index: 4, body: READING_CH4_BODY },
      ],
      nav: true,
      extra: [
        { id: "reading-img", href: "images/reading.png", mediaType: "image/png", bytes: buildPng(8, 8) },
        { id: "reading-css", href: "styles/reading.css", mediaType: "text/css", bytes: READING_CSS },
        { id: "reading-font", href: "fonts/reading.ttf", mediaType: "font/ttf", bytes: READING_FONT_TTF },
      ],
    }),
    manifest: { title: "Reading Fixture Book", author: "Reading Fixture Author", chapters: 4, nav: true, reading: true },
  },
  // 500-chapter stress book, no nav (the spine-derived ToC fallback is
  // exercised by it too) — AC-138 continuous-scroll row.
  {
    name: "epub-stress-500ch.epub",
    bytes: buildEpub({ title: "Stress 500", author: "Stress Author", chapters: 500 }),
    manifest: { title: "Stress 500", author: "Stress Author", chapters: 500 },
  },
  // 1 MB-chapter stress book — AC-138 first-paint row.
  {
    name: "epub-stress-1mb.epub",
    bytes: buildEpub({
      title: "Stress 1MB",
      author: "Stress Author",
      chapters: [
        { index: 1, body: stressChapterBody() },
        { index: 2, body: chapterBody(2) },
      ],
      nav: true,
    }),
    manifest: { title: "Stress 1MB", author: "Stress Author", chapters: 2, nav: true, bigChapter: 1 },
  },
];

const epubManifest = { docs: [] };
for (const { name, bytes, manifest } of epubTier) {
  writeFileSync(join(fixtures, "epub", name), bytes);
  epubManifest.docs.push({ name, ...manifest });
}
writeFileSync(join(fixtures, "epub", "manifest.json"), `${JSON.stringify(epubManifest, null, 2)}\n`);

// AC-131 (SPEC-v0.0.9 §3.8): the mixed-corpus EPUBs, written into the docs
// tier itself — same "Fixture Doc <name>" title and "Fixture Author" author
// the PDFs carry, so every count- and title-derived expectation the 10k
// runner computes stays valid. Single minimal chapter, no cover (the row
// shows the fallback glyph), DEFLATE entries — the zip reader's compressed
// path at scale.
let scaleMixCount = 0;
for (let index = SCALE_MIX_FROM; index <= DOC_COUNT; index += SCALE_MIX_STEP) {
  const name = `doc${String(index).padStart(5, "0")}`;
  writeFileSync(
    join(fixtures, "docs", `${name}.epub`),
    buildEpub({ title: `Fixture Doc ${name}`, author: "Fixture Author", chapters: 1 }),
  );
  scaleMixCount += 1;
}

console.log(
  `fixtures ready: ${DOC_COUNT - scaleMixCount} PDF + ${scaleMixCount} EPUB docs (AC-131 mixed corpus) in fixtures/docs, ${coreNames.length} core in fixtures/core (+ manifest.json), ${Object.keys(special).length} special in fixtures/special, ${multiTier.length} multi-page in fixtures/multi (+ manifest.json), ${epubTier.length} epub in fixtures/epub (+ manifest.json)`
);
