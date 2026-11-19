import type { TrackKey } from '../ipc/bindings/TrackKey';

/** How keys are read. Sort and filter always use the stored Camelot code. */
export type KeyNotation = 'camelot' | 'open_key' | 'musical';

const storageKey = 'resonance-key-notation';

function stored(): KeyNotation {
  const value = localStorage.getItem(storageKey);
  return value === 'open_key' || value === 'musical' ? value : 'camelot';
}

export const keyNotation = $state({ current: stored() });

export function setKeyNotation(notation: KeyNotation) {
  keyNotation.current = notation;
  localStorage.setItem(storageKey, notation);
}

/** The key in the chosen notation. */
export function keyCode(key: TrackKey, notation: KeyNotation = keyNotation.current): string {
  if (notation === 'open_key') return key.open_key;
  if (notation === 'musical') return key.name;
  return key.camelot;
}

/** The reading shown beside the chosen one: the name, or the Camelot code when names are chosen. */
export function keySecond(key: TrackKey, notation: KeyNotation = keyNotation.current): string {
  return notation === 'musical' ? key.camelot : key.name;
}

export function keyDetail(key: TrackKey, notation: KeyNotation = keyNotation.current): string {
  return `${keyCode(key, notation)} · ${keySecond(key, notation)}`;
}
