<script lang="ts">
  import { api } from '../lib/api';
  import { provide } from '../lib/context';
  import { board } from '../lib/flow';
  import { resource } from '../lib/live.svelte';
  import { href, type Tab } from '../lib/route';
  import { at, go, withParams } from '../lib/router.svelte';
  import { here } from '../lib/here.svelte';
  import { project, session } from '../lib/session.svelte';
  import NewItem from '../components/NewItem.svelte';
  import ItemPanel from '../components/ItemPanel.svelte';
  import Overview from './Overview.svelte';
  import Work from './Work.svelte';
  import Plans from './Plans.svelte';
  import Yours from './Yours.svelte';
  import Activity from './Activity.svelte';
  import Settings from './Settings.svelte';

  let { slug, tab }: { slug: string; tab: Tab } = $props();
  let creating = $state(false);

  const graph = resource(() => api.graph(slug));
  const todo = resource(() => api.list('todo', slug));
  const questions = resource(() => api.list('questions', slug));
  const b = $derived(graph.data?.project === slug ? board(graph.data) : undefined);
  const yours = $derived(new Set([...(todo.data ?? []), ...(questions.data ?? [])].map((r) => r.id)).size);
  const open = $derived(at.params.get('i'));

  provide({
    get slug() {
      return slug;
    },
    get row() {
      return project(slug);
    },
    get board() {
      return b;
    },
    item(id: string) {
      const q = new URLSearchParams(at.params);
      q.set('i', id);
      return `${at.path}?${q}`;
    },
  });

  const TABS: { tab: Tab; label: string; key: string }[] = [
    { tab: 'overview', label: 'Overview', key: 'o' },
    { tab: 'work', label: 'Work', key: 'w' },
    { tab: 'plans', label: 'Plans', key: 'p' },
    { tab: 'yours', label: 'Yours', key: 'y' },
    { tab: 'activity', label: 'Activity', key: 'a' },
    { tab: 'settings', label: 'Settings', key: 's' },
  ];

  $effect(() => {
    here.slug = slug;
    here.board = b;
    return () => {
      here.slug = null;
      here.board = undefined;
    };
  });

  const owner = $derived(slug.includes('/') ? slug.slice(0, slug.indexOf('/') + 1) : '');
  const name = $derived(slug.slice(owner.length));

  function closeItem() {
    go(withParams({ i: undefined }));
  }
</script>

<div class="project">
  <header class="head">
    <div class="name">
      <h1><span class="owner">{owner}</span>{name}</h1>
      {#if session.me?.owner}<button class="btn small" onclick={() => (creating = true)}>New item</button>{/if}
    </div>
    <nav class="tabs" aria-label="Pages of {slug}">
      {#each TABS as t (t.tab)}
        <a href={href(slug, t.tab)} class:active={tab === t.tab} aria-current={tab === t.tab ? 'page' : undefined}
          title="g then {t.key}">
          {t.label}
          {#if t.tab === 'yours' && yours > 0}<span class="badge">{yours}</span>{/if}
        </a>
      {/each}
    </nav>
  </header>

  {#if graph.error && !b}
    <p class="error">{graph.error}</p>
  {/if}

  <div class="page" class:split={tab === 'work'}>
    {#key slug}
      {#if tab === 'overview'}<Overview />
      {:else if tab === 'work'}<Work />
      {:else if tab === 'plans'}<Plans />
      {:else if tab === 'yours'}<Yours />
      {:else if tab === 'activity'}<Activity />
      {:else if tab === 'settings'}<Settings />
      {/if}
    {/key}
  </div>

  <NewItem bind:open={creating} />

  {#if open && tab !== 'work'}
    <div class="scrim" role="presentation" onclick={closeItem}></div>
    <aside class="drawer" aria-label="Item {open}">
      {#key open}<ItemPanel id={open} onclose={closeItem} />{/key}
    </aside>
  {/if}
</div>

<style>
  .project {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: space-between;
    gap: 10px 24px;
    padding: 18px 28px 0;
    border-bottom: 1px solid var(--rule);
    background: var(--surface);
  }

  .name {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
    padding-bottom: 12px;
  }

  h1 {
    font-size: 22px;
    letter-spacing: -0.025em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .owner {
    color: var(--faint);
    font-weight: 500;
  }

  .tabs {
    display: flex;
    gap: 2px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tabs a {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px 11px;
    border-bottom: 2px solid transparent;
    color: var(--muted);
    font-weight: 500;
    white-space: nowrap;
  }

  .tabs a:hover {
    color: var(--ink);
  }

  .tabs a.active {
    border-bottom-color: var(--accent);
    color: var(--ink);
    font-weight: 600;
  }

  .badge {
    min-width: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: var(--w-parked);
    color: #fff;
    font-size: 11.5px;
    font-weight: 700;
    text-align: center;
  }

  .page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .page.split {
    overflow: hidden;
  }

  .error {
    margin: 16px 28px 0;
    color: var(--danger);
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: rgb(10 15 30 / 0.28);
  }

  .drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 21;
    width: min(760px, 100vw);
    overflow-y: auto;
    background: var(--paper);
    border-left: 1px solid var(--rule);
    box-shadow: -12px 0 40px rgb(10 15 30 / 0.18);
    animation: slide 0.16s ease-out;
  }

  @keyframes slide {
    from {
      transform: translateX(24px);
      opacity: 0.6;
    }
  }

  @media (max-width: 860px) {
    .head {
      padding: 12px 16px 0 56px;
    }
  }
</style>
