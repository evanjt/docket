<script lang="ts">
  import { api } from '../lib/api';
  import { project } from '../lib/context';
  import { resource } from '../lib/live.svelte';
  import { render } from '../lib/markdown';
  import { act, session } from '../lib/session.svelte';
  import { ago } from '../lib/time';
  import type { Row } from '../lib/types';
  import Word from '../components/Word.svelte';

  const ctx = project();
  const todo = resource(() => api.list('todo', ctx.slug));
  const questions = resource(() => api.list('questions', ctx.slug));
  const decided = resource(() => api.derived(ctx.slug));

  const rows = $derived.by(() => {
    const seen = new Set<string>();
    return [...(todo.data ?? []), ...(questions.data ?? [])].filter((r) => !seen.has(r.id) && seen.add(r.id));
  });

  let drafts = $state<Record<string, string>>({});
  let busy = $state<string | null>(null);
  let expanded = $state(new Set<string>());

  const isQuestion = (r: Row) => ctx.row?.keys.find((k) => k.key === r.key)?.kind === 'decision';
  const links = $derived({ known: (x: string) => !!ctx.board?.nodes.has(x), item: (x: string) => ctx.item(x) });

  async function send(r: Row) {
    const text = (drafts[r.id] ?? '').trim();
    if (!text) return;
    busy = r.id;
    const done = isQuestion(r)
      ? await act('answer', { project: ctx.slug, id: r.id, decision: text }, `Answered ${r.id}`)
      : await act('reply', { project: ctx.slug, id: r.id, note: text }, `Replied to ${r.id}`);
    busy = null;
    if (done) drafts[r.id] = '';
  }

  function more(id: string) {
    const e = new Set(expanded);
    if (e.has(id)) e.delete(id);
    else e.add(id);
    expanded = e;
  }
</script>

<div class="yours">
  <section>
    <h2>Waiting on you <span class="faint">{rows.length}</span></h2>
    {#if rows.length === 0 && todo.data}
      <p class="empty">Nothing is parked on you, and no question is open. The agents have what they need.</p>
    {/if}
    {#each rows as r (r.id)}
      {@const q = isQuestion(r)}
      <article class="ask" class:question={q}>
        <header>
          <a class="id" href={ctx.item(r.id)}>{r.id}</a>
          <Word word={r.word} />
          {#if r.asked_at}<span class="faint">asked {ago(r.asked_at)}</span>{/if}
          {#if r.priority !== 'normal'}<span class="pri">{r.priority}</span>{/if}
        </header>
        <h3><a href={ctx.item(r.id)}>{r.title}</a></h3>
        {#if r.turn_note}<p class="note">{r.turn_note}</p>{/if}
        {#if r.body}
          <div class="prose body" class:clipped={!expanded.has(r.id)}>{@html render(r.body, links)}</div>
          <button class="link" onclick={() => more(r.id)}>{expanded.has(r.id) ? 'Show less' : 'Read the whole item'}</button>
        {/if}
        {#if !q || session.me?.owner}
          <form onsubmit={(e) => { e.preventDefault(); send(r); }}>
            <label class="lbl" for="reply-{r.id}">{q ? 'Your decision' : 'Your reply, sent back to the agents'}</label>
            <textarea id="reply-{r.id}" class="field" bind:value={drafts[r.id]}
              placeholder={q ? 'The option you pick and why, in full' : 'What happened, or what to do next'}
              onkeydown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) send(r); }}></textarea>
            <div class="foot">
              <button class="btn primary small" disabled={!(drafts[r.id] ?? '').trim() || busy === r.id}>
                {q ? 'Answer' : 'Reply'}
              </button>
              <span class="faint hint"><kbd>Ctrl</kbd> <kbd>Enter</kbd></span>
            </div>
          </form>
        {/if}
      </article>
    {/each}
  </section>

  {#if decided.data?.length}
    <section>
      <h2>Decided by agents <span class="faint">for you to look over</span></h2>
      <ul class="derived">
        {#each decided.data as d (d.id)}
          <li>
            <a class="id" href={ctx.item(d.id)}>{d.id}</a>
            <div>
              <a class="title" href={ctx.item(d.id)}>{d.title}</a>
              <p><strong>Chose</strong> {d.chose ?? 'nothing recorded'}</p>
              <p class="faint">From {d.basis}, {ago(d.at)}</p>
            </div>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>

<style>
  .yours {
    display: flex;
    flex-direction: column;
    gap: 32px;
    padding: 22px 28px 48px;
    max-width: 900px;
  }

  h2 {
    font-size: 15px;
    padding-bottom: 6px;
    margin-bottom: 4px;
    border-bottom: 1px solid var(--rule-strong);
  }

  h2 .faint {
    font-weight: 400;
    margin-left: 4px;
  }

  .ask {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 18px 0 22px;
    border-bottom: 1px solid var(--rule);
  }

  .ask header {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
  }

  .pri {
    color: var(--w-building);
    font-weight: 600;
    font-size: 12px;
  }

  h3 {
    font-size: 17px;
    line-height: 1.35;
  }

  .note {
    margin: 0;
    padding: 8px 12px;
    border-left: 3px solid var(--w-parked);
    background: var(--surface);
    border-radius: 0 var(--radius) var(--radius) 0;
  }

  .body {
    font-size: 13.5px;
  }

  .clipped {
    max-height: 9.5em;
    overflow: hidden;
    mask-image: linear-gradient(to bottom, #000 60%, transparent);
  }

  .link {
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 13px;
    cursor: pointer;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 4px;
  }

  .lbl {
    font-size: 12.5px;
    font-weight: 600;
  }

  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .hint {
    font-size: 12px;
  }

  .empty {
    color: var(--muted);
  }

  .derived {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .derived li {
    display: grid;
    grid-template-columns: 56px 1fr;
    gap: 10px;
    padding: 12px 0;
    border-bottom: 1px solid var(--rule);
  }

  .derived p {
    margin: 4px 0 0;
    font-size: 13.5px;
  }

  .derived .title {
    font-weight: 600;
  }

  @media (max-width: 860px) {
    .yours {
      padding: 16px;
    }
  }
</style>
