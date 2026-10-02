<script lang="ts">
  import { api } from '../lib/api';
  import { project } from '../lib/context';
  import { tally, verbWord } from '../lib/flow';
  import { resource } from '../lib/live.svelte';
  import { render } from '../lib/markdown';
  import { ago, stamp } from '../lib/time';
  import type { Kind } from '../lib/types';
  import Actions from './Actions.svelte';
  import Bar from './Bar.svelte';
  import Word from './Word.svelte';

  let { id, onclose }: { id: string; onclose?: () => void } = $props();

  const ctx = project();
  const shown = resource(() => api.show(ctx.slug, id));
  const log = resource(() => api.log(ctx.slug, id));

  const item = $derived(shown.data?.id === id ? shown.data : undefined);
  const node = $derived(ctx.board?.nodes.get(id));
  const kind = $derived<Kind>(node?.kind ?? ctx.row?.keys.find((k) => k.key === item?.key)?.kind ?? 'work');
  const meaning = $derived(ctx.row?.keys.find((k) => k.key === item?.key)?.meaning);
  const links = $derived({ known: (x: string) => !!ctx.board?.nodes.has(x), item: (x: string) => ctx.item(x) });
  const body = $derived(item ? render(item.body, links) : '');
  const parents = $derived(ctx.board?.parents.get(id) ?? []);
  const children = $derived(ctx.board?.children.get(id) ?? []);
  const related = $derived(ctx.board?.related.get(id) ?? item?.related ?? []);
  const cites = $derived((item?.cites ?? []).filter((c) => c.path));
  const t = $derived(ctx.board ? tally(ctx.board, id) : null);
  const host = (h: string | null | undefined) => (h ?? '').split('.')[0];
</script>

<article class="panel" aria-label={id}>
  {#if shown.error && !item}
    <header class="top"><span class="id">{id}</span>{#if onclose}<button class="x" onclick={onclose} aria-label="Hide {id}" title="Hide">×</button>{/if}</header>
    <p class="error">{shown.error}</p>
  {:else if !item}
    <header class="top"><span class="id">{id}</span></header>
    <p class="faint">Reading {id}</p>
  {:else}
    <header class="top">
      <span class="id big">{item.id}</span>
      <Word word={item.word} />
      {#if item.priority !== 'normal'}<span class="tag pri-{item.priority}">{item.priority}</span>{/if}
      {#if item.complexity}<span class="tag">{item.complexity} effort</span>{/if}
      {#if meaning}<span class="faint kind" title={meaning}>{meaning}</span>{/if}
      {#if onclose}<button class="x" onclick={onclose} aria-label="Hide {id}" title="Hide">×</button>{/if}
    </header>
    <h2>{item.title}</h2>

    <dl class="meta">
      <div><dt>Opened</dt><dd title={item.opened_at}>{stamp(item.opened_at)}</dd></div>
      <div><dt>Moved</dt><dd title={item.updated_at}>{ago(item.updated_at)}</dd></div>
      {#if item.group}<div><dt>Group</dt><dd>{item.group}</dd></div>{/if}
      {#if item.theme}<div><dt>Theme</dt><dd>{item.theme}</dd></div>{/if}
      {#if item.claim_branch}
        <div><dt>Claimed</dt><dd><span class="id">{item.claim_branch}</span> on {host(item.claim_on ?? item.claim_host)}, {ago(item.claim_since)}</dd></div>
      {/if}
      {#if t && t.total > 0}<div><dt>Opened items</dt><dd><Bar tally={t} /></dd></div>{/if}
    </dl>

    {#if item.wait_on || item.turn_note || item.decision || item.resolution}
      <div class="standing" style="--c: var(--w-{item.word}, var(--accent))">
        {#if item.wait_on}
          <p><strong>Waits {item.wait_on === 'item' ? 'on' : 'until'}</strong>
            {#if item.wait_on === 'item' && item.wait_ref}<a class="id ref" href={ctx.item(item.wait_ref)}>{item.wait_ref}</a>{:else}{item.wait_ref}{/if}
            {#if item.wait_since}<span class="faint">since {ago(item.wait_since)}</span>{/if}
          </p>
        {/if}
        {#if item.turn_note}<p><strong>{item.word === 'parked' ? 'Asks you' : 'Note'}</strong> {item.turn_note}</p>{/if}
        {#if item.decision}<p><strong>Decided</strong> {item.decision} {#if item.decided_at}<span class="faint">{ago(item.decided_at)}</span>{/if}</p>{/if}
        {#if item.resolution}<p><strong>{item.state === 'dropped' ? 'Dropped' : 'Resolution'}</strong> {item.resolution}
          {#if item.superseded_by}, superseded by <a class="id ref" href={ctx.item(item.superseded_by)}>{item.superseded_by}</a>{/if}</p>{/if}
      </div>
    {/if}

    <Actions {item} {kind} slug={ctx.slug} />

    {#if body}
      <div class="prose body">{@html body}</div>
    {:else}
      <p class="faint">No body.</p>
    {/if}

    {#if parents.length || related.length || children.length}
      <section>
        <h3>Ties</h3>
        <ul class="ties">
          {#each parents as p (p)}{@render tie(p, 'opened it')}{/each}
          {#each related as r (r)}{@render tie(r, 'related')}{/each}
          {#each children as c (c)}{@render tie(c, 'it opened')}{/each}
        </ul>
      </section>
    {/if}

    {#if cites.length}
      <section>
        <h3>Cites</h3>
        <ul class="cites">
          {#each cites as c, i (i)}<li><code>{c.path}{c.line ? `:${c.line}` : ''}</code></li>{/each}
        </ul>
      </section>
    {/if}

    <section>
      <h3>Log</h3>
      {#if log.data}
        <ol class="log">
          {#each [...log.data].reverse() as e (e.seq)}
            <li>
              <span class="when" title={e.at}>{stamp(e.at)}</span>
              <span class="what" style="--c: var(--w-{verbWord(e.kind) ?? 'none'}, var(--ink))">{e.kind}</span>
              <span class="note">{e.note ?? ''}</span>
              <span class="faint where">{host(e.host)}{e.branch ? ` ${e.branch}` : ''}</span>
            </li>
          {/each}
        </ol>
      {:else if log.error}
        <p class="error">{log.error}</p>
      {/if}
    </section>
  {/if}
</article>

{#snippet tie(other: string, how: string)}
  {@const n = ctx.board?.nodes.get(other)}
  <li>
    <span class="how faint">{how}</span>
    <a class="id" href={ctx.item(other)}>{other}</a>
    {#if n}<Word word={n.word} plain /><span class="title">{n.title}</span>{/if}
  </li>
{/snippet}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 20px 24px 40px;
  }

  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  .big {
    font-size: 15px;
  }

  .kind {
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 36ch;
  }

  .tag {
    font-size: 12px;
    padding: 1px 7px;
    border-radius: 10px;
    border: 1px solid var(--rule-strong);
    color: var(--muted);
  }

  .pri-critical {
    border-color: var(--danger);
    color: var(--danger);
    font-weight: 600;
  }

  .pri-high {
    border-color: var(--w-building);
    color: var(--w-building);
  }

  .x {
    margin-left: auto;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    font-size: 22px;
    line-height: 1;
    color: var(--muted);
    cursor: pointer;
  }

  .x:hover {
    background: var(--sunk);
    color: var(--ink);
  }

  h2 {
    font-size: 21px;
    line-height: 1.3;
    max-width: 60ch;
    text-wrap: pretty;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 26px;
    margin: 0;
    font-size: 13px;
  }

  .meta div {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  dt {
    font-size: 12px;
    color: var(--faint);
  }

  dd {
    margin: 0;
  }

  .standing {
    padding: 10px 14px;
    border-left: 3px solid var(--c);
    border-radius: 0 var(--radius) var(--radius) 0;
    background: var(--surface);
    font-size: 13.5px;
  }

  .standing p {
    margin: 0;
  }

  .standing p + p {
    margin-top: 6px;
  }

  .ref {
    color: var(--accent);
  }

  .body {
    padding-top: 4px;
  }

  section {
    padding-top: 12px;
    border-top: 1px solid var(--rule);
  }

  h3 {
    font-size: 13px;
    margin-bottom: 8px;
    color: var(--muted);
  }

  ul,
  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .ties li {
    display: grid;
    grid-template-columns: 76px 56px 92px 1fr;
    align-items: baseline;
    gap: 8px;
    padding: 3px 0;
    font-size: 13px;
  }

  .ties .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .how {
    font-size: 12px;
  }

  .cites {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .cites code {
    font-family: var(--mono);
    font-size: 12px;
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--sunk);
  }

  .log li {
    display: grid;
    grid-template-columns: 112px 74px 1fr auto;
    gap: 10px;
    padding: 4px 0;
    font-size: 12.5px;
    border-bottom: 1px solid var(--rule);
  }

  .log li:last-child {
    border-bottom: 0;
  }

  .when {
    color: var(--muted);
  }

  .what {
    color: var(--c);
    font-weight: 600;
  }

  .note {
    overflow-wrap: anywhere;
  }

  .where {
    font-size: 12px;
    white-space: nowrap;
  }

  .error {
    color: var(--danger);
  }

  @media (max-width: 640px) {
    .panel {
      padding: 16px;
    }

    .log li {
      grid-template-columns: 1fr auto;
    }

    .log .note {
      grid-column: 1 / -1;
    }

    .ties li {
      grid-template-columns: 70px 50px 1fr;
    }

    .ties .title {
      grid-column: 1 / -1;
    }
  }
</style>
