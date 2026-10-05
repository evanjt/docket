<script lang="ts">
  import { api } from '../lib/api';
  import { project } from '../lib/context';
  import { ASIDE, FLOW, PRIORITIES, byId } from '../lib/flow';
  import { resource } from '../lib/live.svelte';
  import { SORTS, applies, filterChanges, nextParams, parseFilter, searchParams, sortRows } from '../lib/filter';
  import { caption, countOf, pageSize } from '../lib/lists';
  import { releaseOf } from '../lib/releases';
  import { at, go, withParams } from '../lib/router.svelte';
  import { act } from '../lib/session.svelte';
  import type { Row } from '../lib/types';
  import ItemPanel from '../components/ItemPanel.svelte';
  import ItemRow from '../components/ItemRow.svelte';

  const ctx = project();

  const LISTS = [
    { name: 'next', label: 'Next' },
    { name: 'todo', label: 'Yours' },
    { name: 'questions', label: 'Questions' },
    { name: 'research', label: 'Research' },
    { name: 'waiting', label: 'Waiting' },
    { name: 'wip', label: 'Claimed' },
    { name: 'derived', label: 'Derived' },
    { name: 'groups', label: 'Groups' },
    { name: 'done', label: 'Done' },
    { name: 'dropped', label: 'Dropped' },
  ];

  type Shape = Pick<Row, 'id' | 'title' | 'word'> & Partial<Row> & { aside?: string };

  const q = $derived(at.params.get('q') ?? '');
  const word = $derived(at.params.get('word'));
  const list = $derived(q || word ? null : (at.params.get('list') ?? 'next'));
  const selected = $derived(at.params.get('i'));
  const filter = $derived(parseFilter(at.params));
  const release = $derived(filter.release);
  const narrowed = $derived(Object.values({ ...filter, sort: '' }).some(Boolean));

  let typed = $state(at.params.get('q') ?? '');
  let searchBox: HTMLInputElement | undefined = $state();
  let marks = $state(new Set<string>());
  let more = $state(0);

  const status = resource(() => api.status(ctx.slug));

  const fetched = resource<Shape[]>(() => {
    if (q) return api.search(ctx.slug, q, 200, searchParams(filter));
    if (word) return null;
    // With a filter chosen, the whole queue is read: next lists the earlier releases first.
    if (list === 'next') return api.next(ctx.slug, narrowed ? 5000 : Math.max(200, pageSize(more, false)), nextParams(filter));
    if (list === 'derived') {
      return api.derived(ctx.slug, pageSize(more, narrowed)).then((ds) =>
        ds.map((d) => ({ id: d.id, title: d.title, word: d.state, turn_note: `${d.chose ?? ''} (from ${d.basis})` })),
      );
    }
    return api.list(list ?? 'next', ctx.slug, pageSize(more, narrowed)).then((rows) =>
      list === 'groups' ? rows.map((r) => ({ ...r, aside: r.group ?? '' })) : rows,
    );
  });

  /** Rows of one word come from the board, which every list route leaves some words out of. */
  const rows = $derived.by<Shape[]>(() => {
    if (word) {
      if (!ctx.board) return [];
      return [...ctx.board.nodes.values()]
        .filter((n) => n.word === word)
        .sort((a, b) => byId(a.id, b.id))
        .map((n) => ({ id: n.id, title: n.title, word: n.word, theme: n.theme }));
    }
    return fetched.data ?? [];
  });

  /** Every item below the plan, story or concept the filter names, for the lists the server does not narrow by it. */
  const below = $derived.by(() => {
    if (!filter.under || !ctx.board) return null;
    const seen = new Set<string>();
    const walk = (id: string) => {
      for (const c of [...(ctx.board!.children.get(id) ?? []), ...(ctx.board!.related.get(id) ?? [])]) {
        if (!seen.has(c)) {
          seen.add(c);
          walk(c);
        }
      }
    };
    walk(filter.under);
    return seen;
  });

  /** The rows that pass the filter, each with its release when the project sets releases, in the sort chosen. */
  const shown = $derived(
    sortRows(
      rows
        .map((r) => ({ ...r, theme: r.theme ?? ctx.board?.nodes.get(r.id)?.theme }))
        .map((r) => ({ ...r, release: releaseOf(r.theme, ctx.releases) }))
        .filter((r) => applies(r, filter, ctx.releases) && (!below || list === 'next' || below.has(r.id))),
      filter.sort,
    ),
  );

  const total = $derived(narrowed || q || word || !list ? null : countOf(list, status.data));
  const cut = $derived(!narrowed && !q && !word && list !== 'next' && list !== 'groups' && list !== 'derived' && total !== null && total > shown.length);

  const loading = $derived(word ? !ctx.board : fetched.loading && !fetched.data);
  const heading = $derived(q ? `Search: ${q}` : word ? `Every item ${word}` : LISTS.find((l) => l.name === list)?.label);

  let timer: ReturnType<typeof setTimeout> | undefined;
  function search(text: string) {
    clearTimeout(timer);
    timer = setTimeout(() => {
      go(withParams({ q: text.trim() || undefined, word: undefined, list: text.trim() ? undefined : 'next' }), true);
    }, 220);
  }

  $effect(() => {
    list;
    q;
    word;
    narrowed;
    more = 0;
  });

  /** The keys the project's items carry, read from the board. */
  const keys = $derived([...new Set([...(ctx.board?.nodes.values() ?? [])].map((n) => n.key))].sort());

  const CHOICES = $derived<{ name: 'key' | 'priority' | 'complexity' | 'state'; label: string; options: readonly string[] }[]>([
    { name: 'key', label: 'Any key', options: keys },
    { name: 'priority', label: 'Any priority', options: PRIORITIES },
    { name: 'complexity', label: 'Any complexity', options: ['high', 'medium', 'low'] },
    { name: 'state', label: 'Any state', options: ['open', 'done', 'dropped'] },
  ]);

  function setFilter(changes: Record<string, string>) {
    go(withParams({ ...changes, i: undefined }), true);
  }

  function choose(id: string) {
    go(withParams({ i: id }), true);
  }

  function move(by: number) {
    if (!shown.length) return;
    const at = shown.findIndex((r) => r.id === selected);
    const next = at < 0 ? 0 : Math.min(shown.length - 1, Math.max(0, at + by));
    choose(shown[next].id);
    document.querySelector(`.rows [data-id="${shown[next].id}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  function toggle(id: string) {
    const m = new Set(marks);
    if (m.has(id)) m.delete(id);
    else m.add(id);
    marks = m;
  }

  async function prioritise(tier: string) {
    const ids = [...marks];
    if (!ids.length || !tier) return;
    const done = await act('priority', { project: ctx.slug, ids, tier }, `${ids.length} set to ${tier}`);
    if (done) marks = new Set();
  }

  function onkey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, select, [contenteditable]')) {
      if (e.key === 'Escape' && t === searchBox) searchBox?.blur();
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === 'j' || e.key === 'ArrowDown') {
      e.preventDefault();
      move(1);
    } else if (e.key === 'k' || e.key === 'ArrowUp') {
      e.preventDefault();
      move(-1);
    } else if (e.key === '/') {
      e.preventDefault();
      searchBox?.focus();
    } else if (e.key === 'x' && selected) {
      toggle(selected);
    }
  }

  let wide = $state(matchMedia('(min-width: 1000px)').matches);

  $effect(() => {
    const query = matchMedia('(min-width: 1000px)');
    const follow = () => (wide = query.matches);
    query.addEventListener('change', follow);
    return () => query.removeEventListener('change', follow);
  });

  $effect(() => {
    if (!selected && shown.length && wide) choose(shown[0].id);
  });
</script>

<svelte:window onkeydown={onkey} />

<div class="work" class:open={!!selected}>
  <div class="left">
    <div class="tools">
      <input
        bind:this={searchBox}
        class="field"
        type="search"
        placeholder="Search titles and bodies"
        bind:value={typed}
        oninput={() => search(typed)}
        aria-label="Search"
      />
      <nav class="lists" aria-label="Lists">
        {#each LISTS as l (l.name)}
          <a href={withParams({ list: l.name, q: undefined, word: undefined, i: undefined })} class:active={list === l.name}
            onclick={() => (typed = '')}>{l.label}</a>
        {/each}
      </nav>
      {#if ctx.releases.length}
        <nav class="lists releases" aria-label="Releases">
          <a href={withParams({ release: undefined, i: undefined })} class:active={!release}>every release</a>
          {#each ctx.releases as r, i (r)}
            <a href={withParams({ release: r, i: undefined })} class:active={release === r}>{r}{i === 0 ? ' (current)' : ''}</a>
          {/each}
        </nav>
      {/if}
      <div class="filters" aria-label="Filters">
        {#each CHOICES as c (c.name)}
          <select aria-label={c.label} value={filter[c.name]} onchange={(e) => setFilter({ [c.name]: e.currentTarget.value })}>
            <option value="">{c.label}</option>
            {#each c.options as o (o)}<option value={o}>{o}</option>{/each}
          </select>
        {/each}
        <select aria-label="Sort" value={filter.sort} onchange={(e) => setFilter({ sort: e.currentTarget.value })}>
          {#each SORTS as o (o.name)}<option value={o.name}>{o.label}</option>{/each}
        </select>
        {#if filter.under}
          <a class="chip" href={withParams({ under: undefined, i: undefined })}>under {filter.under} ×</a>
        {/if}
        {#if narrowed}
          <a class="chip" href={withParams({ ...filterChanges(parseFilter(new URLSearchParams())), sort: filter.sort || undefined, i: undefined })}>clear filters</a>
        {/if}
      </div>
      <nav class="words" aria-label="Words">
        {#each [...FLOW, ...ASIDE, 'done', 'dropped'] as w (w)}
          <a href={withParams({ word: w, q: undefined, list: undefined, i: undefined })} class:active={word === w}
            style="--c: var(--w-{w})" onclick={() => (typed = '')}>{w}</a>
        {/each}
      </nav>
    </div>

    <div class="caption">
      <h2>{heading}</h2>
      <span class="faint">{loading ? 'Reading' : caption(shown.length, total)}</span>
    </div>

    {#if marks.size}
      <div class="bulk">
        <strong>{marks.size} marked</strong>
        <select onchange={(e) => { prioritise(e.currentTarget.value); e.currentTarget.value = ''; }} aria-label="Set priority">
          <option value="">Set priority</option>
          {#each PRIORITIES as p (p)}<option value={p}>{p}</option>{/each}
        </select>
        <button class="btn small" onclick={() => (marks = new Set())}>Clear</button>
      </div>
    {/if}

    {#if fetched.error && !word}
      <p class="error">{fetched.error}</p>
    {:else if !loading && shown.length === 0}
      <p class="empty">
        {q ? 'No item matches. Search reads whole words; try fewer of them.' : 'This list is empty.'}
      </p>
    {/if}
    <ul class="rows">
      {#each shown as r (r.id)}
        <ItemRow row={r} href={withParams({ i: r.id })} selected={r.id === selected} marked={marks.has(r.id)}
          onmark={() => toggle(r.id)} aside={r.aside}
          release={!release && r.release && r.release !== ctx.releases[0] ? r.release : undefined} />
      {/each}
    </ul>
    {#if cut}
      <button class="btn small" onclick={() => (more += 1)}>Show more</button>
    {/if}
  </div>

  <div class="right">
    {#if selected}
      {#key selected}<ItemPanel id={selected} onclose={wide ? undefined : () => go(withParams({ i: undefined }))} />{/key}
    {:else}
      <div class="none">
        <p>Pick an item to read it in full.</p>
        <p class="faint"><kbd>j</kbd> <kbd>k</kbd> move, <kbd>/</kbd> searches, <kbd>x</kbd> marks.</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .work {
    display: grid;
    grid-template-columns: minmax(380px, 44%) 1fr;
    height: 100%;
  }

  .left {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow-y: auto;
    border-right: 1px solid var(--rule);
  }

  .right {
    min-height: 0;
    overflow-y: auto;
  }

  .tools {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 12px 10px;
    position: sticky;
    top: 0;
    z-index: 2;
    background: var(--paper);
    border-bottom: 1px solid var(--rule);
  }

  .lists,
  .words {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .filters select {
    height: 26px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    background: var(--surface);
    font-size: 13px;
  }

  .chip {
    padding: 3px 9px;
    border-radius: 14px;
    background: var(--accent-soft);
    font-size: 13px;
  }

  .releases a {
    font-variant-numeric: tabular-nums;
  }

  .lists a {
    padding: 3px 9px;
    border-radius: 14px;
    font-size: 13px;
    color: var(--muted);
  }

  .lists a:hover {
    background: var(--sunk);
    color: var(--ink);
  }

  .lists a.active {
    background: var(--ink);
    color: var(--paper);
  }

  .words a {
    padding: 1px 8px;
    border: 1px solid var(--rule);
    border-radius: 12px;
    font-size: 12px;
    color: var(--c);
  }

  .words a:hover {
    border-color: var(--c);
  }

  .words a.active {
    background: var(--c);
    border-color: var(--c);
    color: #fff;
  }

  .caption {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 12px 12px 6px;
  }

  h2 {
    font-size: 15px;
  }

  .bulk {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 12px 8px;
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--accent-soft);
    font-size: 13px;
  }

  .bulk select {
    height: 26px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    background: var(--surface);
  }

  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--rule);
  }

  .empty,
  .error {
    padding: 8px 12px;
    color: var(--muted);
  }

  .error {
    color: var(--danger);
  }

  .none {
    display: grid;
    place-content: center;
    height: 100%;
    text-align: center;
    color: var(--muted);
  }

  @media (max-width: 1000px) {
    .work {
      grid-template-columns: 1fr;
    }

    .right {
      display: none;
    }

    .work.open .left {
      display: none;
    }

    .work.open .right {
      display: block;
    }
  }
</style>
