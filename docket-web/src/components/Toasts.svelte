<script lang="ts">
  import { toasts } from '../lib/session.svelte';
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast" class:refused={t.tone === 'refused'}>
      {#if t.tone === 'refused'}<strong>Refused.</strong>{/if}
      {t.text}
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 60;
    max-width: min(440px, calc(100vw - 32px));
  }

  .toast {
    padding: 10px 14px;
    border-radius: var(--radius);
    background: var(--ink);
    color: var(--paper);
    font-size: 13px;
    box-shadow: 0 6px 24px rgb(10 15 30 / 0.25);
  }

  .refused {
    background: var(--danger);
    color: #fff;
  }
</style>
