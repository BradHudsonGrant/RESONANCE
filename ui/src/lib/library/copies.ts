import type { CopyFile } from '../ipc/bindings/CopyFile';
import type { TrackCopies } from '../ipc/bindings/TrackCopies';

/** Every copy of the track's recording: its own files first, then copies on other tracks. */
export function copyGroup(copies: TrackCopies): CopyFile[] {
  return [...copies.files.filter((file) => file.kind !== 'version'), ...copies.copies];
}
