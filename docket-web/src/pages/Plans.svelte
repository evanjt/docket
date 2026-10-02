<script lang="ts">
  import { project } from '../lib/context';
  import { GROUPINGS, byId, plans } from '../lib/flow';
  import Bar from '../components/Bar.svelte';
  import Word from '../components/Word.svelte';

  const ctx = project();
  let unfolded = $state(new Set<string>());
  let showClosed = $state(false);

  const sections = $derived(
    ctx.board ? GROUPINGS.map((g) => ({ ...g, rows: plans(ctx.board!, g.kind) })).filter((s) => s.rows.length) : [],
  );

  function fold(id: string) {
    const u = new Set(unfolded);
    if (u.has(id)) u.delete(id);
    else u.add(id);
    unfolded = u;
  }

  function under(id: string) {
    const b = ctx.board!;
    const node = b.nodes.get(id);
    const standing = node?.kind === 'concept' || node?.kind === 'idea';
    const ids = (standing ? b.related.get(id) : b.children.get(id)) ?? [];
    return ids
      .map((i) => b.nodes.get(i)!)
      .filter((n) => n && (showClosed || (n.word !== 'done' && n.word !== 'dropped')))
      .sort((a, z) => byId(a.id, z.id));
  }
</script>

<div class="plans">
  {#if !ctx.board}
    <p class="faint">Reading the plans</p>
  {:else if sections.length === 0}
    <div class="empty">
      <h2>No open plan</h2>
      <p class="muted">A plan is an <span class="id">A</span> item; the tickets it opens are linked to it, and its audit is due once they are all closed.</p>
    </div>
  {:else}
    <label class="toggle"><input type="checkbox" bind:checked={showClosed} /> Show closed items under each</label>
    {#each sections as s (s.kind)}
      <section>
        <h2>{s.title} <span class="faint">{s.rows.length} open</span></h2>
        <ul>
          {#each s.rows as p (p.node.id)}
            {@const kids = unfolded.has(p.node.id) ? under(p.node.id) : []}
            <li class="plan" class:due={p.due}>
              <div class="line">
                <button class="fold" onclick={() => fold(p.node.id)} aria-expanded={unfolded.has(p.node.id)}
                  aria-label="Show what {p.node.id} opened" disabled={p.tally.total === 0}>
                  {p.tally.total === 0 ? '' : unfolded.has(p.node.id) ? '−' : '+'}
                </button>
                <a class="id" href={ctx.item(p.node.id)}>{p.node.id}</a>
                <a class="title" href={ctx.item(p.node.id)}>{p.node.title}</a>
                <span class="state">
                  {#if p.due}<span class="badge">Audit due</span>{:else}<Word word={p.node.word} plain />{/if}
                </span>
                <span class="bar">{#if p.tally.total}<Bar tally={p.tally} wide />{:else}<span class="faint">opened nothing yet</span>{/if}</span>
              </div>
              {#if kids.length}
                <ul class="kids">
                  {#each kids as k (k.id)}
                    <li>
                      <a class="id" href={ctx.item(k.id)}>{k.id}</a>
                      <Word word={k.word} />
                      <a class="title" href={ctx.item(k.id)}>{k.title}</a>
                    </li>
                  {/each}
                </ul>
              {:else if unfolded.has(p.node.id)}
                <p class="kids faint">Everything it opened is closed.</p>
              {/if}
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
    box-shadow: inset 3px 0 0 var(--w-checking);
  }

  .line {
    display: grid;
    grid-template-columns: 26px 60px 1fr auto 220px;
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
    color: var(--w-checking);
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

  .kids li {
    display: grid;
    grid-template-columns: 56px 92px 1fr;
    align-items: baseline;
    gap: 8px;
    padding: 3px 0;
  }

  .kids li .title {
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

    .state,
    .bar {
      grid-column: 3;
    }

    .kids {
      margin-left: 30px;
    }
  }
</style>
