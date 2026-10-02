<script lang="ts">
  import { api } from '../lib/api';
  import { project } from '../lib/context';
  import { resource } from '../lib/live.svelte';
  import { act, reloadProjects } from '../lib/session.svelte';
  import { ago } from '../lib/time';
  import type { FactSet } from '../lib/types';

  const ctx = project();
  const facts = resource(() => api.facts(ctx.slug));

  let editing = $state<string | null>(null);
  let value = $state('');
  let fresh = $state({ key: '', value: '' });

  function edit(key: string, current: string) {
    editing = key;
    value = current;
  }

  async function save(key: string, v: string) {
    const done = await act<FactSet>('fact', { project: ctx.slug, key, value: v }, v ? `Set ${key}` : `Unset ${key}`);
    if (done) {
      editing = null;
      reloadProjects();
    }
    return done;
  }

  async function add(e: SubmitEvent) {
    e.preventDefault();
    if (await save(fresh.key.trim(), fresh.value.trim())) fresh = { key: '', value: '' };
  }
</script>

<div class="settings">
  <section>
    <h2>Facts</h2>
    <p class="muted">What every agent reads about this project: how to make a worktree, how to merge, the traps.
      An empty value unsets one.</p>
    {#if facts.data}
      <dl class="facts">
        {#each Object.entries(facts.data.skills) as [k, v] (k)}
          <div class="fact">
            <dt class="id">{k}</dt>
            <dd>
              {#if editing === k}
                <form onsubmit={(e) => { e.preventDefault(); save(k, value.trim()); }}>
                  <!-- svelte-ignore a11y_autofocus -->
                  <textarea class="field" bind:value autofocus></textarea>
                  <div class="foot">
                    <button class="btn primary small">Save</button>
                    <button type="button" class="btn small" onclick={() => (editing = null)}>Cancel</button>
                    <button type="button" class="btn small danger" onclick={() => save(k, '')}>Unset</button>
                  </div>
                </form>
              {:else}
                <p>{v}</p>
                <button class="link" onclick={() => edit(k, v)}>Edit</button>
              {/if}
            </dd>
          </div>
        {:else}
          <p class="faint">No fact is set.</p>
        {/each}
      </dl>
      <form class="add" onsubmit={add}>
        <input class="field key" placeholder="Fact" bind:value={fresh.key} aria-label="New fact" />
        <input class="field" placeholder="Value" bind:value={fresh.value} aria-label="Its value" />
        <button class="btn small" disabled={!fresh.key.trim() || !fresh.value.trim()}>Set</button>
      </form>
      {#if facts.data.last_tick}<p class="faint">The loop last ticked {ago(facts.data.last_tick)}.</p>{/if}
    {:else if facts.error}
      <p class="error">{facts.error}</p>
    {/if}
  </section>

  {#if ctx.row}
    <section>
      <h2>Keys</h2>
      <p class="muted">The kinds of item this project holds, and whose turn a new one starts on.</p>
      <table>
        <thead><tr><th>Key</th><th>Kind</th><th>Meaning</th><th>Starts on</th></tr></thead>
        <tbody>
          {#each ctx.row.keys as k (k.key)}
            <tr><td class="id">{k.key}</td><td>{k.kind}</td><td>{k.meaning ?? ''}</td><td>{k.turn === 'user' ? 'you' : (k.turn ?? 'agent')}</td></tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 32px;
    padding: 22px 28px 48px;
    max-width: 980px;
  }

  h2 {
    font-size: 15px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--rule-strong);
  }

  section > p {
    margin: 8px 0 12px;
    font-size: 13.5px;
  }

  .facts {
    margin: 0;
  }

  .fact {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 14px;
    padding: 12px 0;
    border-bottom: 1px solid var(--rule);
  }

  dd {
    margin: 0;
    min-width: 0;
  }

  dd p {
    margin: 0 0 4px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: 13.5px;
  }

  .foot {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 13px;
    cursor: pointer;
  }

  .add {
    display: grid;
    grid-template-columns: 160px 1fr auto;
    gap: 8px;
    margin-top: 14px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13.5px;
  }

  th {
    text-align: left;
    font-size: 12px;
    font-weight: 600;
    color: var(--faint);
    padding: 6px 8px 6px 0;
  }

  td {
    padding: 7px 8px 7px 0;
    border-top: 1px solid var(--rule);
    vertical-align: top;
  }

  .error {
    color: var(--danger);
  }

  @media (max-width: 860px) {
    .settings {
      padding: 16px;
    }

    .fact,
    .add {
      grid-template-columns: 1fr;
      gap: 6px;
    }
  }
</style>
