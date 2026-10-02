<script lang="ts">
  import { api } from '../lib/api';
  import { ASIDE, FLOW } from '../lib/flow';
  import { resource } from '../lib/live.svelte';
  import { href } from '../lib/route';
  import { session } from '../lib/session.svelte';
  import { ago, epoch } from '../lib/time';
  import type { Count, Status } from '../lib/types';

  const counts = resource(() => api.counts());
  const statuses = resource(() =>
    Promise.all(session.projects.map((p) => api.status(p.slug).catch(() => null))).then(
      (all) => new Map(all.filter((s): s is Status => !!s).map((s) => [s.project, s])),
    ),
  );

  let showQuiet = $state(false);
  const WORDS = [...FLOW, ...ASIDE];

  const rows = $derived(
    [...(counts.data ?? [])]
      .filter((c) => showQuiet || c.open > 0)
      .sort((a, b) => (epoch(b.last_event) ?? 0) - (epoch(a.last_event) ?? 0)),
  );
  const quiet = $derived((counts.data ?? []).filter((c) => c.open === 0).length);
  const totals = $derived(
    (counts.data ?? []).reduce((a: { open: number; done: number }, c: Count) => ({ open: a.open + c.open, done: a.done + c.done }), { open: 0, done: 0 }),
  );
  const parked = $derived([...(statuses.data?.values() ?? [])].reduce((a, s) => a + (s.by_word.parked ?? 0), 0));
</script>

<div class="home">
  <header>
    <h1>Every project</h1>
    {#if counts.data}
      <p class="muted">
        {totals.open} open across {counts.data.length - quiet} projects, {totals.done} closed in all.
        {#if parked}<span class="parked">{parked} parked on you.</span>{/if}
      </p>
    {/if}
  </header>

  {#if counts.error}<p class="error">{counts.error}</p>{/if}

  <table>
    <thead>
      <tr>
        <th>Project</th>
        <th class="flowh">Open work by state</th>
        <th class="num">Open</th>
        <th class="num">Parked</th>
        <th class="num">Done</th>
        <th>Last move</th>
      </tr>
    </thead>
    <tbody>
      {#each rows as c (c.slug)}
        {@const s = statuses.data?.get(c.slug)}
        {@const open = s ? WORDS.reduce((a, w) => a + (s.by_word[w] ?? 0), 0) : 0}
        <tr>
          <td><a class="slug" href={href(c.slug)}>{c.slug}</a></td>
          <td class="flowc">
            {#if s && open > 0}
              <a class="mini" href={href(c.slug)} aria-label="{c.slug}: {WORDS.map((w) => `${s.by_word[w] ?? 0} ${w}`).join(', ')}">
                {#each WORDS as w (w)}
                  {#if s.by_word[w]}<span style="flex-grow: {s.by_word[w]}; --c: var(--w-{w})" title="{s.by_word[w]} {w}"></span>{/if}
                {/each}
              </a>
            {/if}
          </td>
          <td class="num strong">{c.open}</td>
          <td class="num" class:parked={(s?.by_word.parked ?? 0) > 0}>{s?.by_word.parked ?? ''}</td>
          <td class="num faint">{c.done}</td>
          <td class="faint when">{c.last_event ? ago(c.last_event) : 'never'}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#if quiet}
    <button class="link" onclick={() => (showQuiet = !showQuiet)}>
      {showQuiet ? 'Hide' : 'Show'} the {quiet} projects with nothing open
    </button>
  {/if}

  <div class="legend">
    {#each WORDS as w (w)}<span style="--c: var(--w-{w})"><i></i>{w}</span>{/each}
  </div>
</div>

<style>
  .home {
    padding: 28px 32px 48px;
    max-width: 1200px;
  }

  h1 {
    font-size: 26px;
    letter-spacing: -0.03em;
  }

  header p {
    margin: 6px 0 22px;
    font-size: 15px;
  }

  .parked {
    color: var(--w-parked);
    font-weight: 600;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    text-align: left;
    font-size: 12px;
    font-weight: 600;
    color: var(--faint);
    padding: 0 10px 8px 0;
    border-bottom: 1px solid var(--rule-strong);
  }

  td {
    padding: 9px 10px 9px 0;
    border-bottom: 1px solid var(--rule);
    white-space: nowrap;
  }

  .num {
    text-align: right;
    width: 64px;
  }

  .strong {
    font-weight: 650;
  }

  .slug {
    font-weight: 550;
  }

  .flowc {
    width: 42%;
  }

  .mini {
    display: flex;
    gap: 2px;
    height: 10px;
    border-radius: 3px;
    overflow: hidden;
  }

  .mini span {
    min-width: 3px;
    background: var(--c);
  }

  .when {
    font-size: 13px;
    padding-left: 16px;
  }

  .link {
    margin-top: 14px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    margin-top: 22px;
    font-size: 12.5px;
    color: var(--muted);
  }

  .legend i {
    display: inline-block;
    width: 10px;
    height: 10px;
    margin-right: 6px;
    border-radius: 2px;
    background: var(--c);
    vertical-align: -1px;
  }

  .error {
    color: var(--danger);
  }

  @media (max-width: 860px) {
    .home {
      padding: 16px 16px 40px 16px;
    }

    h1 {
      padding-left: 40px;
    }

    .flowh,
    .flowc,
    th:nth-child(5),
    td:nth-child(5) {
      display: none;
    }

    .slug {
      white-space: normal;
    }
  }
</style>
