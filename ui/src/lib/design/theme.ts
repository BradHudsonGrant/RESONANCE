export type Theme = 'system' | 'dark' | 'light';

const key = 'resonance-theme';

export function getTheme(): Theme {
  const stored = localStorage.getItem(key);
  return stored === 'dark' || stored === 'light' ? stored : 'system';
}

export function setTheme(theme: Theme) {
  localStorage.setItem(key, theme);
  apply(theme);
}

function apply(theme: Theme) {
  if (theme === 'system') delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
}

/**
 * Calls `repaint` whenever the colour tokens may resolve differently: a theme chosen in Settings
 * or on any element, the system theme, or forced colours. Canvases read tokens once per paint, so
 * they repaint on this. Returns the function that stops listening.
 */
export function onThemeChange(repaint: () => void): () => void {
  const queries = ['(prefers-color-scheme: dark)', '(forced-colors: active)'].map((query) =>
    matchMedia(query),
  );
  const attributes = new MutationObserver(repaint);
  attributes.observe(document.documentElement, {
    attributeFilter: ['data-theme'],
    subtree: true,
  });
  for (const query of queries) query.addEventListener('change', repaint);
  return () => {
    attributes.disconnect();
    for (const query of queries) query.removeEventListener('change', repaint);
  };
}

apply(getTheme());
