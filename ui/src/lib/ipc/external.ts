import { invoke, isTauri } from '@tauri-apps/api/core';

export async function openExternalUrl(url: string): Promise<void> {
  if (!url.startsWith('https://')) {
    throw new Error('Only https:// release links can be opened externally.');
  }
  if (isTauri()) {
    await invoke('open_external_url', { url });
    return;
  }
  window.open(url, '_blank', 'noopener,noreferrer');
}
