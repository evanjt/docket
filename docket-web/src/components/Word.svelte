<script lang="ts">
  import { MEANING } from '../lib/flow';

  let { word, plain = false }: { word: string; plain?: boolean } = $props();
</script>

<span class="word" class:plain style="--c: var(--w-{word.replace(/ /g, '-')}, var(--faint))" title={MEANING[word] ?? word}>
  <i></i>{word}
</span>

<style>
  .word {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    font-weight: 550;
    color: var(--c);
    white-space: nowrap;
  }

  .plain {
    color: var(--muted);
    font-weight: 450;
  }

  i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
    flex: none;
  }
</style>
