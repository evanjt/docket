<script lang="ts">
  import { api } from '../lib/api';
  import { project } from '../lib/context';
  import { MOVE_KINDS, daily, moves, planCount, plans } from '../lib/flow';
  import { jobLine, leadLine, machineUse } from '../lib/fleet';
  import { checkByRelease, checkCounts, forecastLine, releaseTally, staleClaims } from '../lib/releases';
  import { clock, resource } from '../lib/live.svelte';
  import { ITEM_KEYS } from '../lib/newitem';
  import { session } from '../lib/session.svelte';
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
  const lead = resource(() => api.lead(ctx.slug));
  const machines = resource(() => api.machines());
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
  const open = $derived(status.data ? Object.entries(status.data.by_word).filter(([w]) => !['done', 'dropped'].includes(w)).reduce((a, [, n]) => a + n, 0) : null);
  const days = $derived(span.data ? daily(span.data, DAYS, now, span.data.length < MOST) : []);
  const closedWeek = $derived(days.slice(-7).reduce((a, d) => a + d.closed, 0));
  const recent = $derived(
    events.data && ctx.board ? moves(events.data, (rid) => ctx.board?.byRid.get(rid)?.id).slice(0, MOVES) : [],
  );
  const plansOpen = $derived(ctx.board ? plans(ctx.board, 'audit') : []);
  const planTotals = $derived(ctx.board ? planCount(ctx.board) : null);
  const measures = resource(() => (ctx.releases.length ? api.releases(ctx.slug) : null));
  const releases = $derived((measures.data ?? []).map((m, i) => releaseTally(m, i === 0)));
  const forecast = $derived(measures.data?.[0]?.forecast);
  const summary = resource(() => api.summary(ctx.slug));
  const stale = $derived(staleClaims(summary.data?.claims));
  const problems = $derived(checkByRelease(summary.data?.problems ?? [], ctx.releases));
  const counts = $derived(checkCounts(summary.data?.problems ?? [], ctx.slug));
  const OPEN_WORDS = ['ready', 'in progress', 'plans under way', 'waiting on owner', 'blocked', 'held later'];
  const host = (h: string | null | undefined) => (h ?? '').split('.')[0];
  /** The ends jobs reported since the oldest claim was made: all a claim's end can be among. */
  const reports = resource(() => {
    const since = (wip.data ?? []).map((r) => r.claim_since).filter((t): t is string => !!t).sort()[0];
    return since ? api.since(ctx.slug, since, ['job_reported'], MOST) : null;
  });
  const use = $derived(machines.data && wip.data ? machineUse(machines.data, wip.data, reports.data ?? [], session.me?.host ?? '', now) : []);
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
    {#if planTotals}<p class="lede">Plans: {planTotals.open} open, {planTotals.auditDue} audit due</p>{/if}
  {:else if status.error}
    <p class="error">{status.error}</p>
  {:else}
    <div class="ghost"></div>
  {/if}

  <div class="grid">
    <div class="col">
      <section>
        <header><h2>Running now</h2></header>
        {#if lead.data}
          <p class="lead" class:lapsed={lead.data.lapsed} class:none={!lead.data.lead}>{leadLine(lead.data, now)}</p>
        {/if}
        {#if wip.data?.length}
          <ul class="list">
            {#each wip.data as r (r.id)}
              {@const since = epoch(r.claim_since)}
              <li class="run">
                <a class="id" href={ctx.item(r.id)}>{r.id}</a>
                <a class="title" href={ctx.item(r.id)}>{r.title}</a>
                <span class="where"><span class="id branch">{r.claim_branch}</span> on {host(r.claim_on ?? r.claim_host)}{#if jobLine(r)}<span class="job">{jobLine(r)}</span>{/if}</span>
                {#if stale.get(r.id)}<span class="stale" title="No forward event for longer than the stale limit">{stale.get(r.id)}</span>{/if}
                <span class="since" title={r.claim_since ?? ''}>{since ? duration(now - since) : ''}</span>
              </li>
            {/each}
          </ul>
        {:else if wip.data}
          <p class="empty">No item is claimed. Agents claim from Next up.</p>
        {/if}
        {#if use.length}
          <ul class="machines" aria-label="Machines">
            {#each use as m (m.name)}
              <li>
                <span class="name">{m.name}{#if m.here}<span class="here"> (this browser's key)</span>{/if}</span>
                <span class="slots" aria-label="{m.used} of {m.slots} slots in use">
                  {#each Array.from({ length: m.slots }, (_, i) => i < m.used) as busy, i (i)}<i class:busy></i>{/each}
                </span>
                <span class="count">{m.used}/{m.slots}</span>
                {#if m.waiting}<span class="waiting" title="Jobs ended, claims not yet landed">{m.waiting} waiting to land</span>{/if}
                <span class="runners">{m.runners.join(', ')}</span>
                {#each m.limited as l (l.runner)}
                  <span class="limit" title={l.until}>{l.runner} unavailable until {stamp(l.until)}</span>
                {/each}
              </li>
            {/each}
          </ul>
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
          <p class="empty">Nothing is ready. What is left is blocked or waiting on you.</p>
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
          <p class="empty">Nothing is waiting on you and no question is open.</p>
        {/if}
      </section>

      {#if releases.length}
        <section>
          <header>
            <h2>Releases</h2>
            <a class="more" href={href(ctx.slug, 'settings')}>Order</a>
          </header>
          {#if forecast}<p class="forecast faint">{forecastLine(forecast)}</p>{/if}
          <ul class="list compact">
            {#each releases as r (r.name)}
              <li class="release">
                <a class="name" href={href(ctx.slug, 'work', { list: 'next', release: r.name })}>{r.name}</a>
                <span class="state faint">{r.current ? 'current' : ''}</span>
                <span class="counts faint">
                  {OPEN_WORDS.filter((w) => r.words[w]).map((w) => `${r.words[w]} ${w}`).join(', ') || 'nothing open'}
                </span>
                <span class="progress"><Bar tally={r} /></span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if counts.length}
        <section>
          <header><h2>Check</h2></header>
          <ul class="list compact">
            {#each counts as c (c.kind)}
              <li><a href={c.href}>{c.line}</a></li>
            {/each}
          </ul>
          {#each problems as g (g.release)}
            <h3 class="faint">{g.release}</h3>
            <ul class="list compact">
              {#each g.problems as p (`${p.id}${p.by}`)}
                <li>
                  <a class="id" href={ctx.item(p.id ?? '')}>{p.id}</a>
                  <span>held by <a class="id" href={ctx.item(p.by ?? '')}>{p.by}</a> in {p.later}</span>
                </li>
              {/each}
            </ul>
          {/each}
        </section>
      {/if}

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
                    <span class="verb">
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

      {#if ctx.row}
        <section>
          <header><h2>Kinds</h2></header>
          <ul class="keys">
            {#each ITEM_KEYS as k (k.key)}
              {@const counts = status.data?.by_key[k.key]}
              {#if counts}
                <li>
                  <span class="id">{k.key}</span>
                  <span class="meaning">{k.meaning}</span>
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
    color: var(--w-waiting-on-owner);
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
    grid-template-columns: 58px 1fr auto auto auto;
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
    color: var(--w-under-way);
  }

  .release {
    display: grid;
    grid-template-columns: minmax(4ch, auto) minmax(0, 1fr) auto;
    grid-template-areas: 'name state bar' 'counts counts counts';
    align-items: baseline;
    gap: 2px 10px;
  }

  .release .name {
    grid-area: name;
  }

  .release .state {
    grid-area: state;
  }

  .release .progress {
    grid-area: bar;
  }

  .release .name {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .release .counts {
    grid-area: counts;
    font-size: 12.5px;
  }

  .lead {
    margin: 0 0 6px;
    font-size: 13.5px;
  }

  .lead.none {
    color: var(--muted);
  }

  .lead.lapsed {
    color: var(--w-blocked);
  }

  .job {
    margin-left: 8px;
    color: var(--ink);
  }

  .machines {
    margin: 12px 0 0;
    padding: 0;
    list-style: none;
    font-size: 13px;
  }

  .machines li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 4ch auto;
    align-items: center;
    gap: 10px;
    padding: 4px 0;
  }

  .machines .name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .machines .here {
    font-weight: 400;
    color: var(--muted);
  }

  .slots {
    display: flex;
    gap: 2px;
  }

  .slots i {
    width: 8px;
    height: 12px;
    border-radius: 2px;
    background: var(--rule);
  }

  .slots i.busy {
    background: var(--w-under-way);
  }

  .waiting {
    color: var(--muted, inherit);
    font-size: 12.5px;
  }

  .count {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .runners {
    color: var(--muted);
  }

  .limit {
    color: var(--muted);
    font-style: italic;
  }

  .stale {
    font-size: 12.5px;
    color: var(--w-blocked);
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
    color: var(--w-audit-due);
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

  .forecast {
    margin: 4px 0;
    font-size: 13.5px;
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
