<script lang="ts">
  import { onMount } from 'svelte';
  import { follow, tick } from './lib/live.svelte';
  import { resolve, href, type Tab } from './lib/route';
  import { at, go, listen, withParams } from './lib/router.svelte';
  import { session, signIn } from './lib/session.svelte';
  import { applyTheme, loadTheme } from './lib/theme';
  import Help from './components/Help.svelte';
  import Login from './components/Login.svelte';
  import Palette from './components/Palette.svelte';
  import Rail from './components/Rail.svelte';
  import Toasts from './components/Toasts.svelte';
  import Home from './pages/Home.svelte';
  import Project from './pages/Project.svelte';

  let started = $state(false);
  let palette = $state(false);
  let help = $state(false);
  let rail = $state(false);
  let leader = false;

  const route = $derived(resolve(at.path, session.projects.map((p) => p.slug)));
  const current = $derived(route.page === 'project' ? route.slug : null);

  onMount(() => {
    applyTheme(loadTheme());
    const unlisten = listen();
    const untick = tick();
    signIn(session.key).finally(() => (started = true));
    return () => {
      unlisten();
      untick();
    };
  });

  $effect(() => {
    if (!session.me) return;
    const stop = new AbortController();
    follow(stop.signal);
    return () => stop.abort();
  });

  $effect(() => {
    document.title = current ? `${current} · docket` : 'docket';
  });

  const GOTO: Record<string, Tab | 'home'> = { o: 'overview', w: 'work', p: 'plans', y: 'yours', a: 'activity', s: 'settings', h: 'home' };

  function onkey(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      palette = !palette;
      return;
    }
    const typing = (e.target as HTMLElement).closest?.('input, textarea, select, [contenteditable]');
    if (typing || e.metaKey || e.ctrlKey || e.altKey || palette) return;
    if (e.key === 'Escape') {
      if (help) help = false;
      else if (at.params.get('i') && !at.path.endsWith('/work')) go(withParams({ i: undefined }));
      return;
    }
    if (leader) {
      leader = false;
      const to = GOTO[e.key];
      if (to === 'home') go('/ui/');
      else if (to && current) go(href(current, to));
      return;
    }
    if (e.key === 'g') {
      leader = true;
      setTimeout(() => (leader = false), 1200);
    } else if (e.key === '?') {
      help = !help;
    } else if (e.key === ':') {
      e.preventDefault();
      palette = true;
    }
  }
</script>

<svelte:window onkeydown={onkey} />

{#if !started}
  <div class="boot"></div>
{:else if !session.me}
  <Login />
{:else}
  <div class="shell">
    <button class="menu" onclick={() => (rail = !rail)} aria-label="Projects" aria-expanded={rail}>
      <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M3 5h14M3 10h14M3 15h14" /></svg>
    </button>
    {#if rail}<div class="scrim" role="presentation" onclick={() => (rail = false)}></div>{/if}
    <Rail {current} bind:open={rail} />
    <main>
      {#if route.page === 'home'}
        <Home />
      {:else if route.page === 'project'}
        <Project slug={route.slug} tab={route.tab} />
      {:else}
        <div class="lost">
          <h1>No project here</h1>
          <p class="muted">The server holds no project at <code>{route.path}</code>. Pick one from the list, or go to
            <a href="/ui/">every project</a>.</p>
        </div>
      {/if}
    </main>
  </div>
  <Palette bind:open={palette} />
  <Help bind:open={help} />
{/if}
<Toasts />

<style>
  .shell {
    display: flex;
    height: 100%;
  }

  main {
    flex: 1;
    min-width: 0;
    height: 100%;
    overflow-y: auto;
  }

  .boot {
    height: 100%;
  }

  .menu {
    display: none;
  }

  .scrim {
    display: none;
  }

  .lost {
    padding: 40px 32px;
    max-width: 60ch;
  }

  .lost a {
    color: var(--accent);
  }

  @media (max-width: 860px) {
    .menu {
      display: grid;
      place-items: center;
      position: fixed;
      top: 10px;
      left: 10px;
      z-index: 30;
      width: 36px;
      height: 36px;
      border: 1px solid var(--rule);
      border-radius: var(--radius);
      background: var(--surface);
      cursor: pointer;
    }

    .menu svg {
      width: 18px;
      height: 18px;
      stroke: var(--ink);
      stroke-width: 1.6;
    }

    .scrim {
      display: block;
      position: fixed;
      inset: 0;
      z-index: 35;
      background: rgb(10 15 30 / 0.3);
    }
  }
</style>
