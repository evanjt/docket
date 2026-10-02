<script lang="ts">
  import { session, signIn } from '../lib/session.svelte';

  let key = $state('');

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    await signIn(key.trim());
  }
</script>

<div class="wrap">
  <form onsubmit={submit}>
    <h1>docket</h1>
    <p class="muted">
      Paste a key from the server's keys file. It is kept in this browser and sent with every request, so the
      server records your writes against its host.
    </p>
    <label for="key">Key</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="key" class="field" type="password" autocomplete="current-password" bind:value={key} autofocus />
    {#if session.error}<p class="error">{session.error}</p>{/if}
    <button class="btn primary" disabled={!key.trim() || session.checking}>
      {session.checking ? 'Checking' : 'Sign in'}
    </button>
  </form>
</div>

<style>
  .wrap {
    display: grid;
    place-items: center;
    min-height: 100%;
    padding: 16px;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: min(420px, 100%);
    padding: 28px;
    border: 1px solid var(--rule);
    border-radius: 10px;
    background: var(--surface);
  }

  h1 {
    font-size: 28px;
    letter-spacing: -0.03em;
  }

  p {
    margin: 0 0 6px;
  }

  label {
    font-weight: 600;
    font-size: 13px;
  }

  .error {
    color: var(--danger);
  }

  button {
    align-self: flex-start;
    margin-top: 6px;
  }
</style>
