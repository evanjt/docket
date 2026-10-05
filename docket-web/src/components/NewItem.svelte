<script lang="ts">
  import { project } from '../lib/context';
  import { LEVELS, PRIORITIES } from '../lib/flow';
  import { newItemDefaults, newItemRequests } from '../lib/newitem';
  import { go } from '../lib/router.svelte';
  import { act } from '../lib/session.svelte';

  let { open = $bindable(false), from }: { open?: boolean; from?: { id: string; theme?: string | null } } = $props();

  const ctx = project();
  const keys = $derived((ctx.row?.keys ?? []).filter((k) => k.kind !== 'concept' && k.kind !== 'idea'));

  let key = $state('');
  let title = $state('');
  let body = $state('');
  let priority = $state('normal');
  let complexity = $state('');
  let release = $state('');
  let openedBy = $state('');
  let group = $state('');
  let busy = $state(false);

  $effect(() => {
    if (open && !key && keys.length) key = keys[0].key;
  });

  $effect(() => {
    if (!open) return;
    const d = newItemDefaults(from, ctx.releases);
    release = d.release;
    openedBy = d.openedBy;
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    const req = newItemRequests(ctx.slug, { key, title, body, priority, complexity, release, openedBy, group });
    const done = await act<{ item: { id: string } }>('new', req.request, 'Opened');
    const link = done && req.link(done.item.id);
    if (link) await act('link', link, 'Linked');
    busy = false;
    if (!done) return;
    open = false;
    title = '';
    body = '';
    group = '';
    go(ctx.item(done.item.id));
  }
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={() => (open = false)}></div>
  <form class="new" onsubmit={submit} aria-label="New item">
    <h2>New item in {ctx.slug}</h2>
    <label for="new-key">Kind</label>
    <select id="new-key" class="field" bind:value={key}>
      {#each keys as k (k.key)}<option value={k.key}>{k.key}: {k.meaning ?? k.kind}</option>{/each}
    </select>
    <label for="new-title">Title</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="new-title" class="field" bind:value={title} autofocus placeholder="What is wrong or wanted, in one line" />
    <label for="new-body">Body</label>
    <textarea id="new-body" class="field" bind:value={body}
      placeholder={'**Evidence.** What shows it, with file:line.\n**Fix.** What changes.\n**Failing case.** The test that fails first.'}></textarea>
    <div class="pair">
      <label>Priority
        <select class="field" bind:value={priority}>{#each PRIORITIES as p (p)}<option value={p}>{p}</option>{/each}</select>
      </label>
      <label>Effort
        <select class="field" bind:value={complexity}>
          <option value="">unrated</option>
          {#each LEVELS as l (l)}<option value={l}>{l}</option>{/each}
        </select>
      </label>
    </div>
    <div class="pair">
      <label>Release
        <select class="field" bind:value={release}>
          <option value="">current</option>
          {#each ctx.releases as r (r)}<option value={r}>{r}</option>{/each}
        </select>
      </label>
      <label>Group
        <input class="field" bind:value={group} placeholder="none" />
      </label>
    </div>
    <label>Opened by
      <input class="field" bind:value={openedBy} placeholder="an item id, or none" />
    </label>
    <div class="foot">
      <button class="btn primary" disabled={!title.trim() || !key || busy}>Open {key}</button>
      <button type="button" class="btn" onclick={() => (open = false)}>Cancel</button>
    </div>
  </form>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: rgb(10 15 30 / 0.3);
  }

  .new {
    position: fixed;
    top: 8vh;
    left: 50%;
    z-index: 51;
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: min(640px, calc(100vw - 32px));
    max-height: 84vh;
    overflow-y: auto;
    padding: 22px 24px;
    transform: translateX(-50%);
    border-radius: 10px;
    background: var(--surface);
    box-shadow: 0 24px 64px rgb(10 15 30 / 0.3);
  }

  h2 {
    font-size: 17px;
    margin-bottom: 8px;
  }

  label {
    font-size: 12.5px;
    font-weight: 600;
  }

  textarea {
    min-height: 180px;
  }

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .pair label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .foot {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
</style>
