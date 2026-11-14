import { writable } from 'svelte/store';

export type Theme = 'system' | 'dark' | 'light';

const STORAGE_KEY = 'resonance-theme';

function readStoredTheme(): Theme {
  if (typeof localStorage === 'undefined') return 'system';
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'dark' || stored === 'light' || stored === 'system') {
      return stored;
    }
  } catch {
    // Ignore storage errors
  }
  return 'system';
}

export function applyTheme(theme: Theme) {
  if (typeof document === 'undefined') return;
  if (theme === 'system') {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = theme;
  }
}

const initial = readStoredTheme();
const innerStore = writable<Theme>(initial);
let current: Theme = initial;

innerStore.subscribe((val) => {
  current = val;
});

export const themeStore = {
  subscribe: innerStore.subscribe,
  get current(): Theme {
    return current;
  },
  set(theme: Theme) {
    setTheme(theme);
  },
};

export function getTheme(): Theme {
  return current;
}

export function setTheme(theme: Theme) {
  current = theme;
  innerStore.set(theme);
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, theme);
    }
  } catch {
    // Ignore storage errors
  }
  applyTheme(theme);
}

// Initialise theme in DOM if in browser
if (typeof document !== 'undefined') {
  applyTheme(initial);
}
