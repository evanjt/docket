<script lang="ts">
  import type { Tally } from '../lib/flow';

  let { tally, wide = false }: { tally: Tally; wide?: boolean } = $props();
  const done = $derived(tally.total ? (tally.done / tally.total) * 100 : 0);
  const live = $derived(tally.total ? (tally.live / tally.total) * 100 : 0);
</script>

<span class="bar" class:wide role="img" aria-label="{tally.done} of {tally.total} closed, {tally.live} being worked">
  <span class="track">
    <span class="done" style="width: {done}%"></span>
    {#if live > 0}<span class="live" style="width: {live}%"></span>{/if}
  </span>
  <span class="n">{tally.done}/{tally.total}</span>
</span>

<style>
  .bar {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 120px;
  }

  .track {
    display: flex;
    gap: 2px;
    flex: 1;
    height: 6px;
    border-radius: 3px;
    background: var(--sunk);
    overflow: hidden;
  }

  .wide .track {
    height: 8px;
  }

  .done {
    background: var(--w-ready);
    border-radius: 3px;
  }

  .live {
    background: var(--w-building);
    border-radius: 3px;
  }

  .n {
    font-size: 12px;
    color: var(--muted);
    min-width: 4ch;
    text-align: right;
  }
</style>
