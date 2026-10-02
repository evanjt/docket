<script lang="ts">
  import { ASIDE, FLOW, MEANING } from '../lib/flow';
  import { href } from '../lib/route';
  import type { Status } from '../lib/types';

  let { status, slug }: { status: Status; slug: string } = $props();

  const n = (w: string) => status.by_word[w] ?? 0;
  const flow = $derived(FLOW.map((w) => ({ w, n: n(w) })));
  const aside = $derived(ASIDE.map((w) => ({ w, n: n(w) })).filter((s) => s.n > 0));
  const done = $derived(n('done'));
  const open = $derived(flow.reduce((a, s) => a + s.n, 0) + aside.reduce((a, s) => a + s.n, 0));
</script>

<div class="strip" aria-label="Flow of {slug}">
  <div class="track">
    {#each [...flow, ...aside] as s, i (s.w)}
      <a
        class="seg"
        class:empty={s.n === 0}
        class:after={i === flow.length}
        href={href(slug, 'work', { word: s.w })}
        style="--c: var(--w-{s.w}); flex-grow: {Math.max(s.n, open ? open * 0.06 : 1)}"
        title="{s.n} {s.w}: {MEANING[s.w]}"
      >
        <span class="n">{s.n}</span>
        <span class="w">{s.w}</span>
      </a>
    {/each}
  </div>
  <a class="done" href={href(slug, 'work', { word: 'done' })} title="{done} closed">
    <span class="n">{done}</span>
    <span class="w">done</span>
  </a>
</div>

<style>
  .strip {
    display: flex;
    gap: 14px;
    align-items: stretch;
  }

  .track {
    display: flex;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }

  .seg {
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    flex-basis: 0;
    min-width: 76px;
    padding: 12px 12px 10px;
    background: color-mix(in srgb, var(--c) 13%, var(--surface));
    border-bottom: 4px solid var(--c);
    overflow: hidden;
  }

  .seg:first-child {
    border-radius: var(--radius) 0 0 var(--radius);
  }

  .seg:last-child {
    border-radius: 0 var(--radius) var(--radius) 0;
  }

  .seg.after {
    margin-left: 12px;
    border-radius: var(--radius) 0 0 var(--radius);
  }

  .seg.after:last-child {
    border-radius: var(--radius);
  }

  .seg:hover {
    background: color-mix(in srgb, var(--c) 22%, var(--surface));
    color: inherit;
  }

  .seg.empty {
    background: var(--surface);
    border-bottom-color: var(--rule-strong);
  }

  .seg.empty .n {
    color: var(--faint);
  }

  .n {
    font-size: 30px;
    font-weight: 700;
    line-height: 1;
    letter-spacing: -0.03em;
  }

  .w {
    margin-top: 4px;
    font-size: 13px;
    font-weight: 550;
    color: var(--c);
  }

  .seg.empty .w {
    color: var(--faint);
  }

  .done {
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    padding: 12px 4px 14px;
    color: var(--muted);
  }

  .done .n {
    color: var(--muted);
    font-weight: 600;
  }

  .done .w {
    color: var(--faint);
  }

  @media (max-width: 640px) {
    .strip {
      flex-direction: column;
      gap: 6px;
    }

    .track {
      flex-wrap: wrap;
    }

    .seg {
      min-width: calc(33% - 2px);
    }

    .seg.after {
      margin-left: 0;
    }

    .n {
      font-size: 24px;
    }

    .done {
      flex-direction: row;
      align-items: baseline;
      gap: 6px;
      padding: 0 2px;
    }
  }
</style>
