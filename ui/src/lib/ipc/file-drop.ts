import { getCurrentWebview } from '@tauri-apps/api/webview';

/** Files dropped on the desktop window, with where they landed in CSS pixels. */
export type FileDropListener = (paths: string[], x: number, y: number) => void;

const audioFile = /\.(mp3|flac|wav|aiff?|m4a)$/i;

/** Only audio files count; anything else dropped with them is left out. */
export function audioPaths(paths: string[]): string[] {
  return paths.filter((path) => audioFile.test(path));
}

/** Listens for files dropped on the desktop window. A browser page never receives paths. */
export function onFileDrop(listener: FileDropListener): () => void {
  let active = true;
  let unlisten: (() => void) | undefined;
  void getCurrentWebview()
    .onDragDropEvent((event) => {
      if (!active || event.payload.type !== 'drop') return;
      const scale = window.devicePixelRatio || 1;
      const { x, y } = event.payload.position;
      listener(event.payload.paths, x / scale, y / scale);
    })
    .then((stop) => {
      if (active) unlisten = stop;
      else stop();
    });
  return () => {
    active = false;
    unlisten?.();
  };
}
