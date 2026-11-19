import { convertFileSrc, isTauri } from '@tauri-apps/api/core';

/**
 * Where the page loads a stored picture by its hash: the desktop's `artwork` scheme, or the dev
 * server's pass-through to the core. Pictures never travel inside track pages.
 */
export function artworkUrl(hash: string): string {
  return isTauri() ? convertFileSrc(hash, 'artwork') : `/artwork/${hash}`;
}

/** The picture as base64, the way `set_track_artwork` takes it. */
export async function pictureBase64(file: Blob): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = '';
  const chunk = 0x8000;
  for (let start = 0; start < bytes.length; start += chunk) {
    binary += String.fromCharCode(...bytes.subarray(start, start + chunk));
  }
  return btoa(binary);
}
