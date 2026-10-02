<script lang="ts">
  import { FLOW, ASIDE, MEANING } from '../lib/flow';
  import Word from './Word.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  const KEYS: [string, string][] = [
    ['Ctrl K', 'go to an item, a page or a project, or run a verb on the open item'],
    ['g o, g w, g p', 'overview, work, plans'],
    ['g y, g a, g s', 'yours, activity, settings'],
    ['g h', 'every project'],
    ['j, k', 'next and previous row on Work'],
    ['/', 'search on Work'],
    ['x', 'mark the selected row on Work'],
    ['Esc', 'close the open item or this page'],
    ['?', 'this page'],
  ];
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={() => (open = false)}></div>
  <div class="help" role="dialog" aria-label="Keys and words">
    <h2>Keys</h2>
    <dl>
      {#each KEYS as [k, what] (k)}
        <dt>{#each k.split(', ') as part, i (part)}{#if i}, {/if}<kbd>{part}</kbd>{/each}</dt>
        <dd>{what}</dd>
      {/each}
    </dl>
    <h2>Words</h2>
    <dl>
      {#each [...FLOW, ...ASIDE, 'done', 'dropped', 'standing'] as w (w)}
        <dt><Word word={w} /></dt>
        <dd>{MEANING[w]}</dd>
      {/each}
    </dl>
    <button class="btn" onclick={() => (open = false)}>Close</button>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: rgb(10 15 30 / 0.3);
  }

  .help {
    position: fixed;
    top: 50%;
    left: 50%;
    z-index: 51;
    width: min(620px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    overflow-y: auto;
    padding: 22px 26px;
    transform: translate(-50%, -50%);
    border-radius: 10px;
    background: var(--surface);
    box-shadow: 0 24px 64px rgb(10 15 30 / 0.3);
  }

  h2 {
    font-size: 15px;
    margin: 0 0 8px;
  }

  h2 + dl {
    margin-bottom: 20px;
  }

  dl {
    display: grid;
    grid-template-columns: 150px 1fr;
    gap: 6px 14px;
    margin: 0;
    font-size: 13.5px;
  }

  dd {
    margin: 0;
    color: var(--muted);
  }
</style>
