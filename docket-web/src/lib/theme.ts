const KEY = 'docket.theme';

export type Theme = 'light' | 'dark' | 'system';

export function loadTheme(): Theme {
  try {
    const t = localStorage.getItem(KEY);
    if (t === 'light' || t === 'dark') return t;
  } catch {
    // Storage refused: follow the system.
  }
  return 'system';
}

export function applyTheme(t: Theme) {
  if (t === 'system') delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
  try {
    if (t === 'system') localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, t);
  } catch {
    // Storage refused: the choice holds for this tab.
  }
}
