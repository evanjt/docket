<script lang="ts">
  import { api } from '../lib/api';
  import { live, resource } from '../lib/live.svelte';
  import { href } from '../lib/route';
  import { session, signOut } from '../lib/session.svelte';
  import { applyTheme, loadTheme, type Theme } from '../lib/theme';
  import { ago } from '../lib/time';
  import type { Count } from '../lib/types';

  let { current, open = $bindable(false) }: { current: string | null; open?: boolean } = $props();

  const counts = resource(() => api.counts());
  let filter = $state('');
  let theme = $state<Theme>(loadTheme());

  const bySlug = $derived(new Map((counts.data ?? []).map((c: Count) => [c.slug, c])));

  /** Projects under their owner, the part of the slug before its slash. */
  const groups = $derived.by(() => {
    const words = filter.trim().toLowerCase();
    const out = new Map<string, string[]>();
    for (const p of session.projects) {
      if (words && !p.slug.toLowerCase().includes(words)) continue;
      const at = p.slug.indexOf('/');
      const owner = at > 0 ? p.slug.slice(0, at) : '';
      out.set(owner, [...(out.get(owner) ?? []), p.slug]);
    }
    return [...out].sort(([a], [b]) => (a === '' ? 1 : b === '' ? -1 : a.localeCompare(b)));
  });

  function cycleTheme() {
    theme = theme === 'system' ? 'dark' : theme === 'dark' ? 'light' : 'system';
    applyTheme(theme);
  }

  const name = (slug: string) => (slug.includes('/') ? slug.slice(slug.indexOf('/') + 1) : slug);
</script>

<nav class="rail" class:open aria-label="Projects">
  <a class="brand" href="/ui/" onclick={() => (open = false)}>
    <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="1" width="12" height="14" rx="2" /><path d="M5 4.75h6M5 7.75h6M5 10.75h4" /></svg>
    docket
  </a>
  <input class="field filter" placeholder="Filter projects" bind:value={filter} aria-label="Filter projects" />
  <div class="list">
    {#each groups as [owner, slugs] (owner)}
      <div class="group">
        {#if owner}<div class="owner">{owner}</div>{/if}
        {#each slugs as slug (slug)}
          {@const c = bySlug.get(slug)}
          <a
            class="project"
            class:active={slug === current}
            class:quiet={c && c.open === 0}
            href={href(slug)}
            title={c?.last_event ? `last move ${ago(c.last_event)}` : 'no moves yet'}
            onclick={() => (open = false)}
          >
            <span class="name">{name(slug)}</span>
            {#if c && c.open > 0}<span class="n">{c.open}</span>{/if}
          </a>
        {/each}
      </div>
    {:else}
      <p class="faint none">{filter ? 'No project matches.' : 'The server holds no project yet.'}</p>
    {/each}
  </div>
  <footer>
    <span class="who" title={session.me?.owner ? 'An owner key: every verb' : 'An agent key: no new items'}>
      <i class:on={live.connected} title={live.connected ? 'Live' : 'Reconnecting'}></i>
      {session.me?.host.split('.')[0]}
      <span class="faint">{session.me?.owner ? 'owner' : 'agent'}</span>
    </span>
    <span class="links">
      <button class="link" onclick={cycleTheme} title="Colour scheme: system, dark or light">{theme === 'system' ? 'Auto' : theme === 'dark' ? 'Dark' : 'Light'}</button>
      <button class="link" onclick={signOut}>Sign out</button>
    </span>
  </footer>
</nav>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    width: var(--rail);
    height: 100%;
    border-right: 1px solid var(--rule);
    background: var(--surface);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 16px 16px 12px;
    font-size: 19px;
    font-weight: 700;
    letter-spacing: -0.03em;
  }

  .brand svg {
    width: 20px;
    height: 20px;
  }

  .brand rect {
    fill: var(--accent);
  }

  .brand path {
    stroke: var(--surface);
    stroke-width: 1.5;
  }

  .filter {
    margin: 0 12px 8px;
    width: auto;
    padding: 5px 9px;
    font-size: 13px;
  }

  .list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 8px 12px;
  }

  .group + .group {
    margin-top: 10px;
  }

  .owner {
    padding: 4px 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--faint);
  }

  .project {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 5px 8px;
    border-radius: var(--radius);
    font-size: 13.5px;
  }

  .project:hover {
    background: var(--sunk);
    color: var(--ink);
  }

  .project.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }

  .quiet .name {
    color: var(--faint);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .n {
    font-size: 12px;
    color: var(--muted);
  }

  .none {
    padding: 8px;
    font-size: 13px;
  }

  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 10px;
    padding: 10px 14px;
    border-top: 1px solid var(--rule);
    font-size: 12.5px;
  }

  .links {
    display: flex;
    gap: 12px;
  }

  .who {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }

  .who i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--faint);
    flex: none;
  }

  .who i.on {
    background: var(--w-ready);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .link:hover {
    color: var(--accent);
  }

  @media (max-width: 860px) {
    .rail {
      position: fixed;
      inset: 0 auto 0 0;
      z-index: 40;
      transform: translateX(-100%);
      transition: transform 0.18s ease;
    }

    .rail.open {
      transform: none;
      box-shadow: 0 0 40px rgb(10 15 30 / 0.3);
    }
  }
</style>
