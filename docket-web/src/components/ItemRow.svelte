<script lang="ts">
  import type { Row } from '../lib/types';
  import { note } from '../lib/note';
  import Word from './Word.svelte';

  let {
    row,
    href,
    selected = false,
    marked,
    onmark,
    aside,
    release,
  }: {
    row: Pick<Row, 'id' | 'title' | 'word'> & Partial<Row>;
    href: string;
    selected?: boolean;
    marked?: boolean;
    onmark?: () => void;
    aside?: string;
    /** The release the item belongs to, shown when it is not the current one. */
    release?: string;
  } = $props();

  const why = $derived(note(row as Row));
</script>

<li class="row" class:selected data-id={row.id}>
  {#if onmark}
    <input type="checkbox" checked={marked} onchange={onmark} aria-label="Mark {row.id}" />
  {/if}
  <a {href} class="main" aria-current={selected ? 'true' : undefined}>
    <span class="id">{row.id}</span>
    <span class="word"><Word word={row.word} /></span>
    <span class="title">{row.title}</span>
    {#if row.priority && row.priority !== 'normal'}<span class="pri pri-{row.priority}">{row.priority}</span>{/if}
    {#if release}<span class="release" title="Release {release}">{release}</span>{/if}
    {#if aside}<span class="aside faint">{aside}</span>{/if}
    {#if why}<span class="why">{why}</span>{/if}
  </a>
</li>

<style>
  .release {
    flex: none;
    font-size: 11.5px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
    border: 1px solid var(--rule);
    border-radius: 4px;
    padding: 0 5px;
  }

  .row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 0 12px;
    border-bottom: 1px solid var(--rule);
  }

  .row:hover {
    background: var(--surface);
  }

  .row.selected {
    background: var(--accent-soft);
    box-shadow: inset 3px 0 0 var(--accent);
  }

  input {
    margin: 11px 0 0;
    accent-color: var(--accent);
  }

  .main {
    display: grid;
    grid-template-columns: 58px 92px 1fr auto;
    align-items: baseline;
    column-gap: 10px;
    flex: 1;
    min-width: 0;
    padding: 8px 0;
  }

  .main:hover {
    color: inherit;
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row.selected .title {
    font-weight: 550;
  }

  .why {
    grid-column: 3 / -1;
    font-size: 12.5px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pri,
  .aside {
    font-size: 12px;
    white-space: nowrap;
  }

  .pri {
    color: var(--muted);
  }

  .pri-critical {
    color: var(--danger);
    font-weight: 600;
  }

  .pri-high {
    color: var(--w-building);
  }

  @media (max-width: 640px) {
    .main {
      grid-template-columns: 52px 1fr auto;
    }

    .word {
      display: none;
    }

    .why {
      grid-column: 2 / -1;
    }
  }
</style>
