<script lang="ts">
  import { api } from '../lib/api';
  import { project } from '../lib/context';
  import { MOVE_KINDS } from '../lib/flow';
  import { resource } from '../lib/live.svelte';
  import { day, epoch, stamp } from '../lib/time';
  import type { EventRow } from '../lib/types';

  const ctx = project();
  const PAGE = 200;
  let n = $state(PAGE);
  let movesOnly = $state(true);

  const events = resource(() => api.events(ctx.slug, n, movesOnly ? MOVE_KINDS : undefined));

  const days = $derived.by(() => {
    const out: [string, EventRow[]][] = [];
    for (const e of events.data ?? []) {
      const t = epoch(e.at);
      const d = t === null ? '' : day(t);
      const last = out[out.length - 1];
      if (last && last[0] === d) last[1].push(e);
      else out.push([d, [e]]);
    }
    return out;
  });

  const full = (d: string) =>
    new Date(`${d}T12:00:00`).toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });
  const host = (h: string) => h.split('.')[0];
</script>

<div class="activity">
  <div class="bar">
    <label><input type="checkbox" bind:checked={movesOnly} /> Moves only, leaving out edits and links</label>
  </div>
  {#if events.error}<p class="error">{events.error}</p>{/if}
  {#each days as [d, list] (d)}
    <section>
      <h2>{full(d)}</h2>
      <ol>
        {#each list as e (e.seq)}
          {@const node = typeof e.rid === 'number' ? ctx.board?.byRid.get(e.rid) : undefined}
          <li>
            <span class="time" title={e.at}>{stamp(e.at).slice(-5)}</span>
            <span class="verb">{e.kind}</span>
            <span class="what">
              {#if node}<a class="id" href={ctx.item(node.id)}>{node.id}</a> <span class="title">{node.title}</span>{/if}
              {#if e.note}<span class="note">{e.note}</span>{/if}
            </span>
            <span class="who faint">{host(e.host)}{e.branch ? ` ${e.branch}` : ''}</span>
          </li>
        {/each}
      </ol>
    </section>
  {:else}
    {#if events.data}<p class="faint">No moves recorded yet.</p>{/if}
  {/each}
  {#if events.data && events.data.length >= n}
    <button class="btn" onclick={() => (n += PAGE)} disabled={events.loading}>Show older</button>
  {/if}
</div>

<style>
  .activity {
    padding: 18px 28px 48px;
    max-width: 1100px;
  }

  .bar {
    font-size: 13px;
    color: var(--muted);
  }

  section {
    margin-top: 22px;
  }

  h2 {
    position: sticky;
    top: 0;
    padding: 6px 0;
    font-size: 14px;
    background: var(--paper);
    border-bottom: 1px solid var(--rule-strong);
  }

  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: grid;
    grid-template-columns: 48px 84px 1fr auto;
    align-items: baseline;
    gap: 10px;
    padding: 5px 0;
    border-bottom: 1px solid var(--rule);
    font-size: 13px;
  }

  .time {
    color: var(--muted);
  }

  .verb {
    color: var(--c);
    font-weight: 600;
  }

  .what {
    min-width: 0;
  }

  .title {
    color: var(--muted);
  }

  .note {
    display: block;
    overflow-wrap: anywhere;
  }

  .who {
    font-size: 12px;
    white-space: nowrap;
  }

  button {
    margin-top: 18px;
  }

  .error {
    color: var(--danger);
  }

  @media (max-width: 860px) {
    .activity {
      padding: 16px;
    }

    li {
      grid-template-columns: 44px 76px 1fr;
    }

    .who {
      grid-column: 3;
    }
  }
</style>
