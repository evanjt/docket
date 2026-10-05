<script lang="ts">
  import { project } from '../lib/context';
  import { releaseOf } from '../lib/releases';
  import { panel } from '../lib/here.svelte';
  import { act } from '../lib/session.svelte';
  import type { Kind, Offers, Shown } from '../lib/types';
  import { WORDING, editRequest, filled, moveRequest, verbs, type Opened, type Verb } from '../lib/verbs';

  let { item, kind, offers, slug }: { item: Shown; kind: Kind; offers: Offers | undefined; slug: string } = $props();

  type Form = Verb | 'note' | 'edit' | 'link';
  let form = $state<Form | null>(null);
  let text = $state('');
  let extra = $state('');
  let opened: Opened = { title: '', body: '', updated_at: '' };
  let choice = $state('');
  let busy = $state(false);

  const offered = $derived(verbs(offers));
  const offerOf = (f: Form) => offered.find((o) => o.verb === f);

  $effect(() => {
    panel.id = item.id;
    panel.offers = [
      ...offered.map((o) => ({ form: o.verb, label: WORDING[o.verb as Verb].label })),
      { form: 'note', label: 'Add note' },
      { form: 'edit', label: 'Edit' },
      { form: 'link', label: 'Link' },
    ];
    return () => {
      panel.id = null;
      panel.offers = [];
    };
  });

  $effect(() => {
    if (panel.asked && panel.id === item.id) {
      const asked = panel.asked as Form;
      panel.asked = null;
      form = null;
      open(asked);
    }
  });

  function open(f: Form) {
    if (form === f) {
      form = null;
      return;
    }
    form = f;
    opened = { title: item.title, body: item.body, updated_at: item.updated_at };
    text = f === 'edit' ? item.title : '';
    extra = f === 'edit' ? item.body : f === 'start' ? 'main' : '';
    choice = f === 'wait' ? 'until' : f === 'link' ? 'related' : '';
  }

  const common = (branch?: string | null) => ({ project: slug, branch: branch ?? null });
  const given = (s: string) => (s.trim() ? s.trim() : null);

  /** The request a form makes: its verb, its body and the toast once it lands. */
  function request(f: Form): [string, object, string] | null {
    const id = item.id;
    const held = offerOf(f)?.branch;
    switch (f) {
      case 'answer': return ['answer', { ...common(), id, decision: text.trim() }, 'Answered'];
      case 'reply': return ['reply', { ...common(), id, note: text.trim() }, 'Replied'];
      case 'start': return ['start', { ...common(given(extra) ?? 'main'), id }, `Started on ${given(extra) ?? 'main'}`];
      case 'close': return ['close', { ...common(held), id, resolution: given(text) }, 'Closed'];
      case 'release': return ['release', { ...common(held), id, note: given(text) }, 'Unclaimed'];
      case 'resume': return ['resume', { ...common(), id, note: given(text) }, 'Resumed'];
      case 'ask': return ['ask', { ...common(held), id, note: text.trim() }, 'Parked for you'];
      case 'wait':
        return choice === 'on'
          ? ['wait', { ...common(held), id, on: text.trim().toUpperCase() }, `Waiting on ${text.trim().toUpperCase()}`]
          : ['wait', { ...common(held), id, until: text.trim() }, 'Waiting'];
      case 'reopen': return ['reopen', { ...common(), id, why: text.trim() }, 'Reopened'];
      case 'drop': return ['drop', { ...common(held), id, why: text.trim(), superseded_by: given(extra)?.toUpperCase() ?? null }, 'Dropped'];
      case 'note': return ['edit', { ...common(), id, append: text.trim() }, 'Note added'];
      case 'edit': {
        const req = editRequest(opened, text, extra, id, common());
        return req ? ['edit', req, 'Saved'] : null;
      }
      case 'link': return choice === 'parent'
        ? ['parent', { ...common(), a: [id], plan: text.trim().toUpperCase() }, 'Put under the plan']
        : ['link', { ...common(), a: [id], kind: choice, b: text.trim().toUpperCase() }, 'Linked'];
    }
  }

  /** Whether the form holds what its verb cannot go without. */
  const ready = $derived.by(() => {
    if (!form) return false;
    if (form === 'note' || form === 'link') return text.trim().length > 0;
    return filled(offerOf(form), text);
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!form || !ready) return;
    const r = request(form);
    if (!r) {
      form = null;
      return;
    }
    busy = true;
    const done = await act(r[0], r[1], r[2]);
    busy = false;
    if (done) form = null;
  }

  async function setPriority(tier: string) {
    if (tier === item.priority) return;
    await act('priority', { ...common(), ids: [item.id], tier }, `Priority ${tier}`);
  }

  const ctx = project();
  const release = $derived(releaseOf(item.release, ctx.releases));

  async function setRelease(name: string) {
    if (name === release) return;
    await act('edit', moveRequest(item.id, name, common()), `Moved to ${name}`);
  }

  async function setLevel(level: string) {
    if (level === item.complexity) return;
    await act('rate', { ...common(), id: item.id, level }, `Rated ${level}`);
  }

  const PROMPTS: Record<Form, { label: string; field: 'area' | 'line'; placeholder: string }> = {
    answer: { label: 'Decision', field: 'area', placeholder: 'What you decided, and the option it picks' },
    reply: { label: 'Reply', field: 'area', placeholder: 'What happened, for the agent that picks it up' },
    start: { label: 'Branch', field: 'line', placeholder: '' },
    close: { label: 'Resolution', field: 'line', placeholder: 'The sha the work landed as, or what closed it' },
    release: { label: 'Note', field: 'line', placeholder: 'Why it goes back (optional)' },
    resume: { label: 'Note', field: 'line', placeholder: 'What changed (optional)' },
    ask: { label: 'What only you can do', field: 'area', placeholder: 'The question or the step, written in full' },
    wait: { label: '', field: 'line', placeholder: '' },
    reopen: { label: 'Why', field: 'line', placeholder: 'What is still wrong' },
    drop: { label: 'Why', field: 'line', placeholder: 'Why it is no longer wanted' },
    note: { label: 'Note', field: 'area', placeholder: 'Appended to the body under today’s date' },
    edit: { label: 'Title', field: 'line', placeholder: '' },
    link: { label: '', field: 'line', placeholder: 'An id, like A3' },
  };
</script>

<div class="actions">
  <div class="row">
    {#each offered as o, i (o.verb)}
      {@const v = o.verb as Verb}
      <button
        class="btn small"
        class:primary={i === 0 && v !== 'drop'}
        class:danger={v === 'drop'}
        class:on={form === v}
        onclick={() => open(v)}
      >
        {WORDING[v].label}
      </button>
    {/each}
    <span class="gap"></span>
    <button class="btn small" class:on={form === 'note'} onclick={() => open('note')}>Add note</button>
    <button class="btn small" class:on={form === 'edit'} onclick={() => open('edit')}>Edit</button>
    <button class="btn small" class:on={form === 'link'} onclick={() => open('link')}>Link</button>
    <label class="pick">
      <span class="faint">Priority</span>
      <select value={item.priority} onchange={(e) => setPriority(e.currentTarget.value)}>
        {#each offers?.priorities ?? [item.priority] as p (p)}<option value={p}>{p}</option>{/each}
      </select>
    </label>
    <label class="pick">
      <span class="faint">Effort</span>
      <select value={item.complexity ?? ''} onchange={(e) => setLevel(e.currentTarget.value)}>
        {#if !item.complexity}<option value="" disabled>unrated</option>{/if}
        {#each offers?.levels ?? [] as l (l)}<option value={l}>{l}</option>{/each}
      </select>
    </label>
    {#if ctx.releases.length && kind === 'work' && item.state === 'open'}
      <label class="pick">
        <span class="faint">Release</span>
        <select value={release} onchange={(e) => setRelease(e.currentTarget.value)}>
          {#each ctx.releases as r (r)}<option value={r}>{r}</option>{/each}
        </select>
      </label>
    {/if}
  </div>

  {#if form}
    {@const p = PROMPTS[form]}
    <form class="form" onsubmit={submit}>
      {#if form === 'wait'}
        <div class="choice">
          <label><input type="radio" bind:group={choice} value="until" /> Until a condition</label>
          <label><input type="radio" bind:group={choice} value="on" /> On another item</label>
        </div>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="field" bind:value={text} autofocus placeholder={choice === 'on' ? 'An id, like T12' : 'What has to be true first'} />
      {:else if form === 'link'}
        <div class="choice">
          <label><input type="radio" bind:group={choice} value="related" /> {item.id} is related to</label>
          <label><input type="radio" bind:group={choice} value="origin" /> {item.id} was spawned by</label>
          <label><input type="radio" bind:group={choice} value="parent" /> {item.id} is under the plan</label>
        </div>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="field" bind:value={text} autofocus placeholder={p.placeholder} />
      {:else if form === 'edit'}
        <label class="lbl" for="edit-title">Title</label>
        <input id="edit-title" class="field" bind:value={text} />
        <label class="lbl" for="edit-body">Body</label>
        <textarea id="edit-body" class="field tall" bind:value={extra}></textarea>
      {:else if form === 'start'}
        <label class="lbl" for="start-branch">Branch the work is on</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="start-branch" class="field" bind:value={extra} autofocus />
      {:else}
        <label class="lbl" for="verb-text">{p.label}</label>
        {#if p.field === 'area'}
          <!-- svelte-ignore a11y_autofocus -->
          <textarea id="verb-text" class="field" bind:value={text} placeholder={p.placeholder} autofocus
            onkeydown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) e.currentTarget.form?.requestSubmit(); }}></textarea>
        {:else}
          <!-- svelte-ignore a11y_autofocus -->
          <input id="verb-text" class="field" bind:value={text} placeholder={p.placeholder} autofocus />
        {/if}
        {#if form === 'drop'}
          <label class="lbl" for="drop-by">Superseded by</label>
          <input id="drop-by" class="field" bind:value={extra} placeholder="An id (optional)" />
        {/if}
      {/if}
      <div class="foot">
        <button class="btn primary small" disabled={!ready || busy}>
          {form === 'note' ? 'Add note' : form === 'edit' ? 'Save' : form === 'link' ? 'Link' : form === 'ask' ? 'Park for me' : WORDING[form as Verb]?.label}
        </button>
        <button type="button" class="btn small" onclick={() => (form = null)}>Cancel</button>
        {#if PROMPTS[form].field === 'area'}<span class="faint hint"><kbd>Ctrl</kbd> <kbd>Enter</kbd> to send</span>{/if}
      </div>
    </form>
  {/if}
</div>

<style>
  .actions {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .gap {
    flex: 1;
  }

  .btn.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }

  .btn.primary.on {
    color: var(--accent-ink);
    background: var(--accent);
  }

  .pick {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12.5px;
  }

  select {
    height: 26px;
    padding: 0 4px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    background: var(--surface);
    font-size: 13px;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--rule);
    border-radius: var(--radius);
    background: var(--paper);
  }

  .lbl {
    font-size: 12.5px;
    font-weight: 600;
  }

  .tall {
    min-height: 260px;
    font-family: var(--mono);
    font-size: 12.5px;
  }

  .choice {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    font-size: 13px;
  }

  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .hint {
    margin-left: auto;
    font-size: 12px;
  }
</style>
