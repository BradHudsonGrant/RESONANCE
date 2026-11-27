import { convertFileSrc, isTauri } from '@tauri-apps/api/core';

import { readOverviews } from '../mix/frames';

/**
 * Where the page loads the waveform overviews of some tracks: the desktop's `waveforms` scheme,
 * or the dev server's pass-through to the core. Overviews never travel inside track pages.
 */
export function overviewsUrl(trackIds: readonly number[]): string {
  const query = `?tracks=${trackIds.join(',')}`;
  return isTauri() ? `${convertFileSrc('', 'waveforms')}${query}` : `/waveforms${query}`;
}

/** The overviews of `trackIds`, in the same order, null for a track without one yet. */
export async function fetchOverviews(trackIds: readonly number[]): Promise<(Uint8Array | null)[]> {
  const response = await fetch(overviewsUrl(trackIds));
  if (!response.ok) throw new Error(`Waveforms did not load (${response.status}).`);
  return readOverviews(await response.arrayBuffer());
}
