import { ROOT } from './route';

/** Where the page stands: its path and its query, kept in step with the address bar. */
export const at = $state({
  path: location.pathname,
  params: new URLSearchParams(location.search),
});

function sync() {
  at.path = location.pathname;
  at.params = new URLSearchParams(location.search);
}

export function go(to: string, replace = false) {
  if (to === location.pathname + location.search) return;
  if (replace) history.replaceState(null, '', to);
  else history.pushState(null, '', to);
  sync();
}

/** The current address with some query parameters set, or removed by `undefined`. */
export function withParams(changes: Record<string, string | undefined>): string {
  const q = new URLSearchParams(location.search);
  for (const [k, v] of Object.entries(changes)) {
    if (v) q.set(k, v);
    else q.delete(k);
  }
  const s = q.toString();
  return s ? `${location.pathname}?${s}` : location.pathname;
}

/** Follows the back button, and turns clicks on links inside the app into navigation without a reload. */
export function listen(): () => void {
  const onPop = () => sync();
  const onClick = (e: MouseEvent) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const a = (e.target as Element | null)?.closest?.('a');
    if (!a || a.target || a.hasAttribute('download')) return;
    const url = new URL(a.href, location.href);
    if (url.origin !== location.origin || !url.pathname.startsWith(ROOT)) return;
    e.preventDefault();
    go(url.pathname + url.search);
  };
  addEventListener('popstate', onPop);
  document.addEventListener('click', onClick);
  return () => {
    removeEventListener('popstate', onPop);
    document.removeEventListener('click', onClick);
  };
}
