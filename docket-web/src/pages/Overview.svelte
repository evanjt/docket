<script lang="ts">
  import { api } from '../lib/api';
  import { project } from '../lib/context';
  import { MOVE_KINDS, daily, moves, plans, verbWord } from '../lib/flow';
  import { clock, resource } from '../lib/live.svelte';
  import { href } from '../lib/route';
  import { ago, duration, epoch, stamp } from '../lib/time';
  import Bar from '../components/Bar.svelte';
  import DailyChart from '../components/DailyChart.svelte';
  import FlowStrip from '../components/FlowStrip.svelte';
  import ItemRow from '../components/ItemRow.svelte';
  import Word from '../components/Word.svelte';

  const ctx = project();
  const READ = 300;
  const DAYS = 14;
  /** The most open and close events the chart reads, so a burst of thousands cannot stall the page. */
  const MOST = 20_000;
  const NEXT = 8;
  const MOVES = 6;

  const status = resource(() => api.status(ctx.slug));
  const wip = resource(() => api.list('wip', ctx.slug));
  const next = resource(() => api.next(ctx.slug, NEXT));
  const todo = resource(() => api.list('todo', ctx.slug));
  const questions = resource(() => api.list('questions', ctx.slug));
  const events = resource(() => api.events(ctx.slug, READ, MOVE_KINDS));
  const start = $derived.by(() => {
    const d = new Date(clock.now * 1000);
    d.setHours(0, 0, 0, 0);
    d.setDate(d.getDate() - (DAYS - 1));
    return d.toISOString().replace(/\.\d+Z$/, 'Z');
  });
  const span = resource(() => api.since(ctx.slug, start, ['opened', 'closed', 'dropped'], MOST));

  const now = $derived(clock.now);
  const yours = $derived.by(() => {
    const seen = new Set<string>();
    return [...(todo.data ?? []), ...(questions.data ?? [])].filter((r) => !seen.has(r.id) && seen.add(r.id));
  });
  const open = $derived(status.data ? Object.entries(status.data.by_word).filter(([w]) => !['done', 'dropped', 'standing'].includes(w)).reduce((a, [, n]) => a + n, 0) : null);
  const days = $derived(span.data ? daily(span.data, DAYS, now, span.data.length < MOST) : []);
  const closedWeek = $derived(days.slice(-7).reduce((a, d) => a + d.closed, 0));
  const recent = $derived(
    events.data && ctx.board ? moves(events.data, (rid) => ctx.board?.byRid.get(rid)?.id).slice(0, MOVES) : [],
  );
  const plansOpen = $derived(ctx.board ? plans(ctx.board, 'audit') : []);
  const host = (h: string | null | undefined) => (h ?? '').split('.')[0];
  const COUNTED = ['claimed', 'released'];
</script>

<div class="overview">
  {#if status.data}
    <FlowStrip status={status.data} slug={ctx.slug} />
    <p class="lede">
      {open} open{#if days.length >= 7}, {closedWeek} {closedWeek === 1 ? 'close' : 'closes'} in the last 7 days{/if}.
      {#if wip.data?.length}{wip.data.length} claimed right now.{:else if wip.data}Nothing is claimed right now.{/if}
      {#if yours.length}<a href={href(ctx.slug, 'yours')} class="you">{yours.length} waiting on you.</a>{/if}
    </p>
  {:else if status.error}
    <p class="error">{status.error}</p>
  {:else}
    <div class="ghost"></div>
  {/if}

  <div class="grid">
    <div class="col">
      <section>
        <header><h2>Running now</h2></header>
        {#if wip.data?.length}
          <ul class="list">
            {#each wip.data as r (r.id)}
              {@const since = epoch(r.claim_since)}
              <li class="run">
                <a class="id" href={ctx.item(r.id)}>{r.id}</a>
                <a class="title" href={ctx.item(r.id)}>{r.title}</a>
                <span class="where"><span class="id branch">{r.claim_branch}</span> on {host(r.claim_on ?? r.claim_host)}</span>
                <span class="since" title={r.claim_since ?? ''}>{since ? duration(now - since) : ''}</span>
              </li>
            {/each}
          </ul>
        {:else if wip.data}
          <p class="empty">No item is claimed. Agents claim from Next up.</p>
        {/if}
      </section>

      <section>
        <header>
          <h2>Next up</h2>
          <a class="more" href={href(ctx.slug, 'work', { list: 'next' })}>Whole queue</a>
        </header>
        {#if next.data?.length}
          <ul class="list rows">
            {#each next.data as r (r.id)}<ItemRow row={r} href={ctx.item(r.id)} />{/each}
          </ul>
        {:else if next.data}
          <p class="empty">Nothing is ready. What is left is blocked, parked on you or not yet scoped.</p>
        {/if}
      </section>

      <section>
        <header><h2>Opened and closed</h2></header>
        {#if days.length}
          <DailyChart {days} />
        {:else if span.data}
          <p class="empty">No moves recorded yet.</p>
        {/if}
      </section>
    </div>

    <div class="col side">
      <section>
        <header>
          <h2>Waiting on you</h2>
          {#if yours.length}<a class="more" href={href(ctx.slug, 'yours')}>Answer</a>{/if}
        </header>
        {#if yours.length}
          <ul class="list compact">
            {#each yours.slice(0, 6) as r (r.id)}
              <li>
                <a class="id" href={ctx.item(r.id)}>{r.id}</a>
                <a class="title" href={ctx.item(r.id)}>{r.title}</a>
              </li>
            {/each}
          </ul>
          {#if yours.length > 6}<a class="more below" href={href(ctx.slug, 'yours')}>{yours.length - 6} more</a>{/if}
        {:else if todo.data}
          <p class="empty">Nothing is parked on you and no question is open.</p>
        {/if}
      </section>

      <section>
        <header>
          <h2>Plans</h2>
          <a class="more" href={href(ctx.slug, 'plans')}>All</a>
        </header>
        {#if plansOpen.length}
          <ul class="list compact">
            {#each plansOpen.slice(0, 6) as p (p.node.id)}
              <li class="plan">
                <a class="id" href={ctx.item(p.node.id)}>{p.node.id}</a>
                <a class="title" href={ctx.item(p.node.id)}>{p.node.title}</a>
                {#if p.due}<span class="due">audit due</span>{/if}
                <span class="progress"><Bar tally={p.tally} /></span>
              </li>
            {/each}
          </ul>
        {:else if ctx.board}
          <p class="empty">No open plan.</p>
        {/if}
      </section>

      <section>
        <header>
          <h2>Recent moves</h2>
          <a class="more" href={href(ctx.slug, 'activity')}>Full log</a>
        </header>
        {#if recent.length}
          <ol class="moves">
            {#each recent as m (m.at)}
              <li>
                <span class="when" title={stamp(m.at)}>{ago(m.at, now)}</span>
                <span class="verbs">
                  {#each m.verbs as [verb, ids] (verb)}
                    <span class="verb" style="--c: var(--w-{verbWord(verb) ?? 'none'}, var(--ink))">
                      <span class="v">{verb}</span>
                      {#if COUNTED.includes(verb)}{ids.length}{:else}
                        {#each ids as id (id)}<a class="id" href={ctx.item(id)}>{id}</a>{' '}{/each}
                      {/if}
                    </span>
                  {/each}
                </span>
              </li>
            {/each}
          </ol>
        {:else if events.data}
          <p class="empty">No moves recorded yet.</p>
        {/if}
      </section>

      {#if ctx.row?.keys.length}
        <section>
          <header><h2>Kinds</h2></header>
          <ul class="keys">
            {#each ctx.row.keys as k (k.key)}
              {@const counts = status.data?.by_key[k.key]}
              {#if counts}
                <li>
                  <span class="id">{k.key}</span>
                  <span class="meaning">{k.meaning ?? k.kind}</span>
                  <span class="words">
                    {#each Object.entries(counts).filter(([w]) => !['done', 'dropped'].includes(w)) as [w, n] (w)}
                      <a href={href(ctx.slug, 'work', { word: w })} title="{n} {w}"><Word word={w} plain />{n}</a>
                    {/each}
                  </span>
                </li>
              {/if}
            {/each}
          </ul>
        </section>
      {/if}
    </div>
  </div>
</div>

<style>
  .overview {
    padding: 22px 28px 48px;
    max-width: 1440px;
  }

  .lede {
    margin: 14px 0 0;
    font-size: 15px;
    color: var(--muted);
  }

  .lede .you {
    color: var(--w-parked);
    font-weight: 600;
  }

  .ghost {
    height: 82px;
    border-radius: var(--radius);
    background: var(--sunk);
  }

  .grid {
    display: grid;
    grid-template-columns: minmax(0, 1.55fr) minmax(300px, 1fr);
    gap: 32px;
    margin-top: 26px;
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: 30px;
    min-width: 0;
  }

  section header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 8px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--rule-strong);
  }

  h2 {
    font-size: 15px;
  }

  .more {
    font-size: 12.5px;
    color: var(--accent);
  }

  .more.below {
    display: inline-block;
    margin-top: 6px;
  }

  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows :global(.row) {
    padding: 0;
  }

  .run {
    display: grid;
    grid-template-columns: 58px 1fr auto auto;
    align-items: baseline;
    gap: 12px;
    padding: 8px 0;
    border-bottom: 1px solid var(--rule);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .where {
    font-size: 12.5px;
    color: var(--muted);
    white-space: nowrap;
  }

  .branch {
    font-weight: 500;
    color: var(--w-building);
  }

  .since {
    font-weight: 600;
    min-width: 6ch;
    text-align: right;
  }

  .compact li {
    display: grid;
    grid-template-columns: 56px 1fr;
    align-items: baseline;
    gap: 4px 10px;
    padding: 6px 0;
    border-bottom: 1px solid var(--rule);
    font-size: 13.5px;
  }

  .plan .progress {
    grid-column: 2;
  }

  .plan .due {
    grid-column: 2;
    justify-self: start;
    font-size: 12px;
    font-weight: 600;
    color: var(--w-checking);
  }

  .moves {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .moves li {
    display: grid;
    grid-template-columns: 92px 1fr;
    gap: 10px;
    padding: 6px 0;
    border-bottom: 1px solid var(--rule);
    font-size: 13px;
  }

  .when {
    color: var(--faint);
    font-size: 12.5px;
  }

  .verbs {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 12px;
  }

  .verb .v {
    color: var(--c);
    font-weight: 600;
    margin-right: 4px;
  }

  .keys {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .keys li {
    display: grid;
    grid-template-columns: 44px 1fr;
    gap: 2px 10px;
    padding: 6px 0;
    border-bottom: 1px solid var(--rule);
    font-size: 13px;
  }

  .meaning {
    color: var(--muted);
  }

  .words {
    grid-column: 2;
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .words a {
    display: inline-flex;
    gap: 5px;
    font-size: 12.5px;
  }

  .empty {
    margin: 4px 0;
    color: var(--muted);
    font-size: 13.5px;
  }

  .error {
    color: var(--danger);
  }

  @media (max-width: 1100px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 860px) {
    .overview {
      padding: 16px 16px 40px;
    }

    .run {
      grid-template-columns: 52px 1fr auto;
    }

    .run .where {
      grid-column: 2;
    }
  }
</style>
