<script lang="ts">
  import { here, panel } from '../lib/here.svelte';
  import { findItems, score } from '../lib/palette';
  import { TABS, href } from '../lib/route';
  import { at, go } from '../lib/router.svelte';
  import { session } from '../lib/session.svelte';
  import Word from './Word.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  interface Choice {
    key: string;
    label: string;
    hint?: string;
    word?: string;
    id?: string;
    run: () => void;
  }

  let typed = $state('');
  let at_ = $state(0);
  let input: HTMLInputElement | undefined = $state();

  const itemHref = (slug: string, id: string) => {
    const tab = at.path.startsWith(href(slug)) ? undefined : 'work';
    if (tab) return href(slug, 'work', { i: id });
    const q = new URLSearchParams(at.params);
    q.set('i', id);
    return `${at.path}?${q}`;
  };

  const choices = $derived.by<Choice[]>(() => {
    const out: Choice[] = [];
    const t = typed.trim();
    const slug = here.slug;
    if (panel.id) {
      const id = panel.id;
      for (const o of panel.offers) {
        if (!t || score(t, o.label) > 0 || score(t, `${o.form} ${id}`) > 0) {
          out.push({ key: `v:${o.form}`, label: `${o.label} ${id}`, hint: 'on the open item', run: () => (panel.asked = o.form) });
        }
      }
    }
    if (slug && here.board) {
      for (const n of findItems(t, here.board.nodes.values(), 7)) {
        out.push({ key: `i:${n.id}`, label: n.title, id: n.id, word: n.word, run: () => go(itemHref(slug, n.id)) });
      }
    }
    if (slug && t) {
      out.push({ key: 'search', label: `Search ${slug} for “${t}”`, run: () => go(href(slug, 'work', { q: t })) });
    }
    if (slug) {
      for (const tab of TABS) {
        if (score(t, tab) > 0 || !t) {
          out.push({ key: `t:${tab}`, label: `Go to ${tab}`, hint: slug, run: () => go(href(slug, tab)) });
        }
      }
    }
    const projects = session.projects
      .map((p) => [score(t, p.slug), p.slug] as const)
      .filter(([s]) => s > 0)
      .sort((a, b) => b[0] - a[0])
      .slice(0, t ? 6 : 4);
    for (const [, p] of projects) out.push({ key: `p:${p}`, label: p, hint: 'project', run: () => go(href(p)) });
    out.push({ key: 'home', label: 'Every project', hint: 'home', run: () => go('/ui/') });
    return out.filter((c) => c.key !== 'home' || !t || score(t, 'home every project') > 0);
  });

  $effect(() => {
    if (open) {
      typed = '';
      at_ = 0;
      queueMicrotask(() => input?.focus());
    }
  });

  $effect(() => {
    void typed;
    at_ = 0;
  });

  function pick(c: Choice | undefined) {
    if (!c) return;
    open = false;
    c.run();
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || (e.key === 'n' && e.ctrlKey)) {
      e.preventDefault();
      at_ = Math.min(choices.length - 1, at_ + 1);
    } else if (e.key === 'ArrowUp' || (e.key === 'p' && e.ctrlKey)) {
      e.preventDefault();
      at_ = Math.max(0, at_ - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      pick(choices[at_]);
    } else if (e.key === 'Escape') {
      open = false;
    }
  }
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={() => (open = false)}></div>
  <div class="palette" role="dialog" aria-label="Go to">
    <input
      bind:this={input}
      bind:value={typed}
      onkeydown={onkey}
      placeholder={here.slug ? `An id, words of a title, a page or a project` : 'A project'}
      aria-label="Go to"
      role="combobox"
      aria-expanded="true"
      aria-controls="palette-list"
      aria-activedescendant="choice-{at_}"
    />
    <ul id="palette-list" role="listbox">
      {#each choices as c, i (c.key)}
        <li id="choice-{i}" role="option" aria-selected={i === at_}>
          <button class:on={i === at_} onclick={() => pick(c)} onmousemove={() => (at_ = i)}>
            {#if c.id}<span class="id">{c.id}</span>{/if}
            {#if c.word}<span class="w"><Word word={c.word} /></span>{/if}
            <span class="label">{c.label}</span>
            {#if c.hint}<span class="hint">{c.hint}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
    <footer><kbd>↑</kbd> <kbd>↓</kbd> to move, <kbd>Enter</kbd> to go, <kbd>Esc</kbd> to close</footer>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: rgb(10 15 30 / 0.3);
  }

  .palette {
    position: fixed;
    top: 12vh;
    left: 50%;
    z-index: 51;
    width: min(640px, calc(100vw - 32px));
    transform: translateX(-50%);
    border: 1px solid var(--rule-strong);
    border-radius: 10px;
    background: var(--surface);
    box-shadow: 0 24px 64px rgb(10 15 30 / 0.3);
    overflow: hidden;
  }

  input {
    width: 100%;
    padding: 16px 18px;
    border: 0;
    border-bottom: 1px solid var(--rule);
    background: none;
    font-size: 16px;
    outline: none;
  }

  ul {
    max-height: 52vh;
    margin: 0;
    padding: 6px;
    list-style: none;
    overflow-y: auto;
  }

  button {
    display: flex;
    align-items: baseline;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    text-align: left;
    cursor: pointer;
  }

  button.on {
    background: var(--accent-soft);
  }

  .id {
    min-width: 48px;
  }

  .w {
    min-width: 80px;
  }

  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint {
    font-size: 12px;
    color: var(--faint);
  }

  footer {
    padding: 8px 14px;
    border-top: 1px solid var(--rule);
    font-size: 12px;
    color: var(--faint);
  }
</style>
