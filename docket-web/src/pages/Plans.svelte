<script lang="ts">
  import { project } from '../lib/context';
  import { remainingHref } from '../lib/filter';
  import { releaseOf } from '../lib/releases';
  import { at, go, withParams } from '../lib/router.svelte';
  import { GROUPINGS, byId, planWord, plans, tally } from '../lib/flow';
  import Bar from '../components/Bar.svelte';
  import Word from '../components/Word.svelte';

  const ctx = project();
  let unfolded = $state(new Set<string>());
  let showClosed = $state(false);

  const text = $derived((at.params.get('q') ?? '').trim().toLowerCase());
  const release = $derived(at.params.get('release') ?? '');
  let typed = $state(at.params.get('q') ?? '');

  const sections = $derived(
    ctx.board
      ? GROUPINGS.map((g) => ({
          ...g,
          rows: plans(ctx.board!, g.kind).filter(
            (p) =>
              (!text || `${p.node.id} ${p.node.title}`.toLowerCase().includes(text)) &&
              (!release || releaseOf(p.node.release, ctx.releases) === release),
          ),
        })).filter((s) => s.rows.length)
      : [],
  );

  function fold(id: string) {
    const u = new Set(unfolded);
    if (u.has(id)) u.delete(id);
    else u.add(id);
    unfolded = u;
  }

  /** How deep the tree unfolds below a plan, as the TUI folds it. */
  const DEEPEST = 4;

  function under(id: string) {
    const b = ctx.board!;
    const ids = b.children.get(id) ?? [];
    return ids
      .map((i) => b.nodes.get(i)!)
      .filter((n) => n && (showClosed || (n.word !== 'done' && n.word !== 'dropped')))
      .sort((a, z) => byId(a.id, z.id));
  }
</script>

{#snippet tree(id: string, depth: number)}
  {@const kids = under(id)}
  {#if kids.length}
    <ul class="kids">
      {#each kids as k (k.id)}
        {@const t = tally(ctx.board!, k.id)}
        {@const foldable = t.total > 0 && depth < DEEPEST}
        <li>
          <div class="kid">
            <button class="fold" onclick={() => fold(k.id)} aria-expanded={unfolded.has(k.id)}
              aria-label="Show what {k.id} opened" disabled={!foldable}>
              {foldable ? (unfolded.has(k.id) ? '−' : '+') : ''}
            </button>
            <a class="id" href={ctx.item(k.id)}>{k.id}</a>
            <Word word={k.word} />
            <a class="title" href={ctx.item(k.id)}>{k.title}</a>
            <span class="bar">{#if t.total}<Bar tally={t} />{/if}</span>
          </div>
          {#if foldable && unfolded.has(k.id)}{@render tree(k.id, depth + 1)}{/if}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="kids faint">Everything under it is closed.</p>
  {/if}
{/snippet}

<div class="plans">
  {#if !ctx.board}
    <p class="faint">Reading the plans</p>
  {:else if sections.length === 0}
    <div class="empty">
      <h2>No open plan</h2>
      <p class="muted">A plan is an <span class="id">A</span> item; the tickets it opens are linked to it, and its audit is due once they are all closed.</p>
    </div>
  {:else}
    <div class="filters">
      <input class="field" type="search" placeholder="Filter plans by id or title" aria-label="Filter plans" bind:value={typed}
        oninput={() => go(withParams({ q: typed.trim() || undefined }), true)} />
      {#if ctx.releases.length}
        <a class="chip" class:active={!release} href={withParams({ release: undefined })}>every release</a>
        {#each ctx.releases as r, i (r)}
          <a class="chip" class:active={release === r} href={withParams({ release: r })}>{r}{i === 0 ? ' (current)' : ''}</a>
        {/each}
      {/if}
    </div>
    <label class="toggle"><input type="checkbox" bind:checked={showClosed} /> Show closed items under each</label>
    {#each sections as s (s.kind)}
      <section>
        <h2>{s.title} <span class="faint">{s.rows.length} open</span></h2>
        <ul>
          {#each s.rows as p (p.node.id)}
            <li class="plan" class:due={p.due}>
              <div class="line">
                <button class="fold" onclick={() => fold(p.node.id)} aria-expanded={unfolded.has(p.node.id)}
                  aria-label="Show what {p.node.id} opened" disabled={p.tally.total === 0}>
                  {p.tally.total === 0 ? '' : unfolded.has(p.node.id) ? '−' : '+'}
                </button>
                <a class="id" href={ctx.item(p.node.id)}>{p.node.id}</a>
                <a class="title" href={ctx.item(p.node.id)}>{p.node.title}</a>
                {#if releaseOf(p.node.release, ctx.releases)}<span class="release faint">{releaseOf(p.node.release, ctx.releases)}</span>{/if}
                <a class="remaining" href={remainingHref(ctx.slug, p.node.id)}>remaining work</a>
                <span class="state">
                  {#if p.due}<span class="badge">Audit due</span>{:else if planWord(p)}<Word word={planWord(p) ?? ''} plain />{/if}
                </span>
                <span class="bar">{#if p.tally.total}<Bar tally={p.tally} wide />{:else}<span class="faint">nothing under it yet</span>{/if}</span>
              </div>
              {#if unfolded.has(p.node.id)}{@render tree(p.node.id, 1)}{/if}
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  {/if}
</div>

<style>
  .plans {
    padding: 22px 28px 48px;
    max-width: 1200px;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
  }

  .chip {
    padding: 3px 9px;
    border-radius: 14px;
    font-size: 13px;
    color: var(--muted);
  }

  .chip.active {
    background: var(--ink);
    color: var(--paper);
  }

  .release,
  .remaining {
    font-size: 12.5px;
  }

  .toggle {
    display: inline-flex;
    gap: 6px;
    font-size: 13px;
    color: var(--muted);
  }

  section {
    margin-top: 22px;
  }

  h2 {
    font-size: 15px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--rule-strong);
  }

  h2 .faint {
    font-weight: 400;
    font-size: 13px;
    margin-left: 6px;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .plan {
    border-bottom: 1px solid var(--rule);
  }

  .plan.due {
    box-shadow: inset 3px 0 0 var(--w-audit-due);
  }

  .line {
    display: grid;
    grid-template-columns: 26px 60px 1fr auto auto auto 220px;
    align-items: center;
    gap: 10px;
    padding: 9px 4px;
  }

  .fold {
    width: 22px;
    height: 22px;
    border: 1px solid var(--rule-strong);
    border-radius: 4px;
    background: var(--surface);
    color: var(--muted);
    cursor: pointer;
    line-height: 1;
  }

  .fold:disabled {
    border-color: transparent;
    background: none;
    cursor: default;
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .badge {
    font-size: 12px;
    font-weight: 650;
    color: var(--w-audit-due);
  }

  .bar {
    display: flex;
    font-size: 12.5px;
  }

  .bar :global(.bar) {
    flex: 1;
  }

  .kids {
    margin: 0 0 10px 100px;
    padding-left: 12px;
    border-left: 2px solid var(--rule);
    font-size: 13px;
  }

  .kid {
    display: grid;
    grid-template-columns: 26px 56px 92px 1fr 160px;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }

  .kids .kids {
    margin: 0 0 4px 34px;
  }

  .kid .title {
    font-weight: 400;
  }

  .empty {
    max-width: 60ch;
  }

  @media (max-width: 860px) {
    .plans {
      padding: 16px;
    }

    .line {
      grid-template-columns: 26px 54px 1fr;
    }

    .release,
    .remaining,
    .state,
    .bar {
      grid-column: 3;
    }

    .kids {
      margin-left: 30px;
    }

    .kid {
      grid-template-columns: 26px 54px auto 1fr;
    }

    .kid .bar {
      grid-column: 2 / -1;
    }
  }
</style>
