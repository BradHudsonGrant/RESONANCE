import type { TrackAudio } from '../ipc/bindings/TrackAudio';

/** `5:01` for minutes and seconds, `1:02:03` once a track runs past an hour. */
export function duration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return '';
  const seconds = Math.round(ms / 1000);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = String(seconds % 60).padStart(2, '0');
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, '0')}:${rest}` : `${minutes}:${rest}`;
}

/** One decimal place, so a column of tempos lines up. */
export function tempo(bpm: number | null): string {
  return bpm === null ? '' : bpm.toFixed(1);
}

export function fileSize(bytes: number): string {
  if (bytes < 1_000_000) return `${Math.max(1, Math.round(bytes / 1000))} kB`;
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

export function sampleRate(hertz: number): string {
  return `${(hertz / 1000).toLocaleString('en-GB', { maximumFractionDigits: 1 })} kHz`;
}

export function trackCount(total: number): string {
  return total === 1 ? '1 track' : `${total.toLocaleString('en-GB')} tracks`;
}

/** Format line for the inspector header, such as `WAV · 44.1 kHz · 24-bit`. */
export function audioSummary(format: string, audio: TrackAudio | null): string {
  const parts = [format.toUpperCase()];
  if (audio?.sample_rate) parts.push(sampleRate(audio.sample_rate));
  if (audio?.bit_depth) parts.push(`${audio.bit_depth}-bit`);
  else if (audio?.bitrate_kbps) parts.push(`${audio.bitrate_kbps} kbps`);
  if (audio?.channels === 1) parts.push('Mono');
  return parts.join(' · ');
}

/** Stable positive seed from a track's path, so its dot-wave thumbnail stays the same across runs. */
export function trackSeed(identity: string): number {
  let hash = 2166136261;
  for (let index = 0; index < identity.length; index++) {
    hash ^= identity.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 1 || 1;
}
