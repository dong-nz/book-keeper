<script lang="ts">
  // Tag picker (UI-02 MODIFIED, SPEC-v0.0.14 §3.2 as revised by DR-437): the
  // Details tags section. The document's current tags render as removable
  // chips; "+ Add tag" opens an inline add-picker — a filter field (the
  // search affordance) and the tag list as rows in the released list-row
  // language: membership is the active state (check glyph, accent-quiet
  // ground), the matched substring is marked in accent, a Create row is
  // pinned when the text has no exact match (the released create path), and
  // ↑/↓ move a cursor row while the field keeps typing focus (AC-205). The
  // pending set belongs to the editor: every mutation goes out through
  // `onchange`; Save sends the full set through the unchanged update path.
  // The picker is an inline panel, not an overlay — Escape closes it first,
  // decided by the editor's released onEscapeKeydown override (the `open`
  // binding is how it knows).
  import { library } from "$lib/state/library.svelte";
  import { t } from "$lib/i18n";
  import Icon from "./Icon.svelte";
  import {
    chipsFor,
    filterTags,
    inlineTagAction,
    matchSegments,
    moveCursorIndex,
    toggleTagId,
  } from "$lib/tagpicker";

  let {
    tagIds,
    onchange,
    open = $bindable(false),
  }: {
    tagIds: number[];
    onchange: (ids: number[]) => void;
    open?: boolean;
  } = $props();

  let query = $state("");
  let cursor = $state(-1);

  const chips = $derived(chipsFor(tagIds, library.tags));
  const listed = $derived(filterTags(library.tags, query));
  // The Create row shows exactly when Enter in the field would create
  // (inlineTagAction kind "create") — one decision, two affordances (§3.2).
  const createName = $derived(
    inlineTagAction(query, library.tags).kind === "create" ? query.trim() : "",
  );

  function toggle(id: number) {
    onchange(toggleTagId(tagIds, id));
  }

  async function submitQuery() {
    const action = inlineTagAction(query, library.tags);
    if (action.kind === "none") return;
    if (action.kind === "join") {
      if (!tagIds.includes(action.id)) onchange(toggleTagId(tagIds, action.id));
      query = "";
      return;
    }
    const created = await library.createTag(action.name);
    if (created) {
      onchange(toggleTagId(tagIds, created.id));
      query = "";
    }
  }

  function fieldKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      // DR-437 (AC-205): the arrows steer the cursor row, never the caret.
      event.preventDefault();
      cursor = moveCursorIndex(listed.length, cursor, event.key === "ArrowDown" ? 1 : -1);
      return;
    }
    if (event.key !== "Enter") return;
    // Enter in the picker field is the picker's, never the form's Save:
    // with a cursor active it activates the cursor row, otherwise it keeps
    // its join/create meaning (unchanged).
    event.preventDefault();
    if (cursor >= 0 && cursor < listed.length) {
      toggle(listed[cursor].id);
      return;
    }
    void submitQuery();
  }
</script>

<div class="tag-picker">
  {#if chips.length > 0}
    <div class="chips">
      {#each chips as tag (tag.id)}
        <span class="tag-chip">
          <span class="tag-chip-name">{tag.name}</span>
          <button
            class="tag-chip-x"
            type="button"
            aria-label={t("details.tagRemove", { name: tag.name })}
            onclick={() => toggle(tag.id)}
          >
            <Icon name="close" />
          </button>
        </span>
      {/each}
    </div>
  {/if}

  <button class="control add-tag" type="button" aria-expanded={open} onclick={() => (open = !open)}>
    <Icon name="plus" />
    <span>{t("details.tagAdd")}</span>
  </button>

  {#if open}
    <div class="picker">
      <div class="picker-field">
        <Icon name="search" />
        <input
          class="field"
          type="text"
          placeholder={t("details.tagFilter")}
          aria-label={t("details.tagFilter")}
          bind:value={query}
          oninput={() => (cursor = -1)}
          onkeydown={fieldKeydown}
        />
      </div>
      <div class="picker-list">
        {#if createName}
          <button class="create-row" type="button" onclick={() => void submitQuery()}>
            <span class="row-glyph"><Icon name="plus" /></span>
            <span class="row-name">{t("details.tagCreate", { name: createName })}</span>
          </button>
        {/if}
        {#each listed as tag, index (tag.id)}
          <button
            class="pick-row"
            type="button"
            aria-pressed={tagIds.includes(tag.id)}
            data-assigned={tagIds.includes(tag.id) || undefined}
            data-cursor={cursor === index || undefined}
            onclick={() => toggle(tag.id)}
          >
            <span class="row-glyph">
              {#if tagIds.includes(tag.id)}<Icon name="check" />{/if}
            </span>
            <span class="row-name">
              {#each matchSegments(tag.name, query) as segment}
                <span class:matched={segment.match}>{segment.text}</span>
              {/each}
            </span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .tag-picker {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 6px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .tag-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 100%;
    padding: 2px 4px 2px 8px;
    border: 1px solid var(--border-hairline);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    font-size: var(--text-12);
  }

  .tag-chip-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-chip-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 2px;
    border: none;
    border-radius: var(--radius-control);
    background: none;
    color: var(--text-tertiary);
    cursor: pointer;
  }

  .tag-chip-x:hover {
    color: var(--text-primary);
  }

  .add-tag {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
  }

  .picker {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  /* DR-437: the filter field carries the catalog search glyph. */
  .picker-field {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-tertiary);
  }

  .picker-field > .field {
    flex: 1;
    min-width: 0;
  }

  .picker-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 160px;
    overflow: auto;
  }

  /* DR-437 (§3.2): the released list-row language (the TagRow conventions) —
     no checkboxes; membership is the D11 active state; the cursor row is the
     hover ground plus a hairline. */
  .pick-row,
  .create-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-width: 0;
    height: var(--row-control-height);
    padding: 0 8px;
    border: none;
    border-radius: var(--radius-control);
    background: none;
    color: var(--text-secondary);
    font-size: var(--text-13);
    text-align: left;
    cursor: pointer;
  }

  .pick-row:hover,
  .create-row:hover {
    background: var(--surface-2);
  }

  .pick-row[data-assigned] {
    /* D11: text on a 12 % tint of its own color takes the hover value. */
    color: var(--accent-hover);
    background: var(--accent-quiet);
  }

  .pick-row[data-cursor] {
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--border-hairline);
  }

  .row-glyph {
    flex: none;
    display: inline-flex;
    width: 16px;
    justify-content: center;
  }

  .create-row .row-glyph {
    color: var(--accent);
  }

  .create-row {
    color: var(--text-primary);
  }

  .row-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* DR-437: the matched substring is marked color-only. */
  .matched {
    color: var(--accent);
  }
</style>
