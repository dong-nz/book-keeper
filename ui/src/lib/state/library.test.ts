// D38 (SPEC-v0.0.14 §3.7, AC-201 unit half): the delete path re-reads the
// tag list alongside the documents, so the Browse-menu and Tags-manager
// counts match actual membership right after a deletion. The mock API records
// the `tagsList` call after a successful delete — and NOT after a failed one
// (a failed delete leaves the library untouched, §3.7). The bulk tag path
// carries the same contract per DR-436.

import { beforeEach, describe, expect, it, vi } from "vitest";

const { libraryDeleteDocument, libraryDeleteDocumentAndFile, libraryBulkTags, tagsList } = vi.hoisted(() => ({
  libraryDeleteDocument: vi.fn(() => Promise.resolve()),
  libraryDeleteDocumentAndFile: vi.fn(() => Promise.resolve()),
  libraryBulkTags: vi.fn(() => Promise.resolve()),
  tagsList: vi.fn(() => Promise.resolve([{ id: 1, name: "Invoices", count: 2 }])),
}));

vi.mock("../../api", () => ({
  libraryDeleteDocument,
  libraryDeleteDocumentAndFile,
  libraryBulkTags,
  tagsList,
  appGetSettings: vi.fn(() => Promise.resolve({})),
  appSetSetting: vi.fn(() => Promise.resolve()),
  librarySearch: vi.fn(() => Promise.resolve({ total: 0, documents: [] })),
}));

import { library } from "./library.svelte";

describe("library store delete (D38: tag counts stay true to membership)", () => {
  beforeEach(() => {
    libraryDeleteDocument.mockClear();
    libraryDeleteDocumentAndFile.mockClear();
    tagsList.mockClear();
    tagsList.mockResolvedValue([{ id: 1, name: "Invoices", count: 2 }]);
    library.rows = [];
    library.total = 0;
  });

  it("a successful delete re-reads the tag list after the command returns", async () => {
    const ok = await library.delete([7], false);
    expect(ok).toBe(true);
    expect(libraryDeleteDocument).toHaveBeenCalledTimes(1);
    expect(tagsList).toHaveBeenCalledTimes(1);
    // the re-read lands in the store — the counts the Browse menu renders
    expect(library.tags).toEqual([{ id: 1, name: "Invoices", count: 2 }]);
    // ordering fact: the tags re-read happens after the delete command
    expect(tagsList.mock.invocationCallOrder[0]).toBeGreaterThan(
      libraryDeleteDocument.mock.invocationCallOrder[0],
    );
  });

  it("the delete-with-files variant re-reads tags too", async () => {
    const ok = await library.delete([7], true);
    expect(ok).toBe(true);
    expect(libraryDeleteDocumentAndFile).toHaveBeenCalledTimes(1);
    expect(tagsList).toHaveBeenCalledTimes(1);
  });

  it("a failed delete leaves the tag list alone (no re-read)", async () => {
    libraryDeleteDocument.mockRejectedValueOnce({ kind: "io", message: "file lock" });
    const ok = await library.delete([7], false);
    expect(ok).toBe(false);
    expect(tagsList).not.toHaveBeenCalled();
  });
});

describe("library store bulkTags (D38 as widened by DR-436: the bulk path keeps counts true)", () => {
  beforeEach(() => {
    libraryBulkTags.mockClear();
    tagsList.mockClear();
    tagsList.mockResolvedValue([{ id: 1, name: "Invoices", count: 3 }]);
    library.rows = [];
    library.total = 0;
  });

  it("a bulk tag assignment re-reads the tag list after the command returns", async () => {
    await library.bulkTags([1], []);
    expect(libraryBulkTags).toHaveBeenCalledTimes(1);
    expect(tagsList).toHaveBeenCalledTimes(1);
    expect(library.tags).toEqual([{ id: 1, name: "Invoices", count: 3 }]);
    // ordering fact: the tags re-read happens after the bulk command
    expect(tagsList.mock.invocationCallOrder[0]).toBeGreaterThan(
      libraryBulkTags.mock.invocationCallOrder[0],
    );
  });

  it("a failed bulk assignment leaves the tag list alone (no re-read)", async () => {
    libraryBulkTags.mockRejectedValueOnce({ kind: "io", message: "file lock" });
    await library.bulkTags([1], []);
    expect(tagsList).not.toHaveBeenCalled();
  });
});
