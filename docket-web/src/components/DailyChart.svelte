<script lang="ts">
  import type { Day } from '../lib/flow';

  let { days }: { days: Day[] } = $props();

  let hover = $state<number | null>(null);
  const top = $derived(Math.max(1, ...days.map((d) => Math.max(d.opened, d.closed + d.dropped))));
  const shown = $derived(hover === null ? null : days[hover]);
  const label = (d: string) => {
    const [, m, day] = d.split('-');
    return `${Number(day)}/${Number(m)}`;
  };
  const totals = $derived(days.reduce((a, d) => ({ o: a.o + d.opened, c: a.c + d.closed + d.dropped }), { o: 0, c: 0 }));
</script>

<figure class="chart">
  <div class="legend">
    <span><i class="opened"></i>Opened</span>
    <span><i class="closed"></i>Closed or dropped</span>
    <span class="readout" aria-live="polite">
      {#if shown}
        {label(shown.day)}: {shown.opened} opened, {shown.closed} closed{shown.dropped ? `, ${shown.dropped} dropped` : ''}
      {:else}
        {totals.o} opened and {totals.c} closed over {days.length} days
      {/if}
    </span>
  </div>
  <div class="plot" role="img" aria-label="Opened and closed items per day">
    {#each days as d, i (d.day)}
      <div
        class="day"
        class:hover={hover === i}
        role="presentation"
        onmouseenter={() => (hover = i)}
        onmouseleave={() => (hover = null)}
      >
        <div class="bars">
          <span class="bar opened" style="height: {(d.opened / top) * 100}%"></span>
          <span class="bar closed" style="height: {((d.closed + d.dropped) / top) * 100}%"></span>
        </div>
        <span class="tick">{i % 2 === days.length % 2 || days.length <= 8 ? label(d.day) : ''}</span>
      </div>
    {/each}
  </div>
  <table class="sr">
    <caption>Opened and closed per day</caption>
    <thead><tr><th>Day</th><th>Opened</th><th>Closed</th><th>Dropped</th></tr></thead>
    <tbody>
      {#each days as d (d.day)}<tr><td>{d.day}</td><td>{d.opened}</td><td>{d.closed}</td><td>{d.dropped}</td></tr>{/each}
    </tbody>
  </table>
</figure>

<style>
  .chart {
    margin: 0;
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 16px;
    font-size: 12.5px;
    color: var(--muted);
  }

  .legend i {
    display: inline-block;
    width: 10px;
    height: 10px;
    margin-right: 6px;
    border-radius: 2px;
    vertical-align: -1px;
  }

  .readout {
    margin-left: auto;
    color: var(--ink);
  }

  .plot {
    display: flex;
    gap: 2px;
    height: 120px;
    margin-top: 12px;
    border-bottom: 1px solid var(--rule-strong);
  }

  .day {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    border-radius: 4px 4px 0 0;
  }

  .day.hover {
    background: var(--sunk);
  }

  .bars {
    display: flex;
    align-items: flex-end;
    justify-content: center;
    gap: 2px;
    flex: 1;
    padding: 0 3px;
  }

  .bar {
    flex: 1;
    max-width: 12px;
    min-height: 0;
    border-radius: 4px 4px 0 0;
  }

  .opened,
  .legend .opened {
    background: var(--rule-strong);
  }

  .closed,
  .legend .closed {
    background: var(--accent);
  }

  .tick {
    position: absolute;
    top: calc(100% + 4px);
    left: 50%;
    transform: translateX(-50%);
    font-size: 11px;
    color: var(--faint);
    white-space: nowrap;
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
