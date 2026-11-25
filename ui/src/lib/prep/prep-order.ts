/** How Prep keeps its list steady while the library changes underneath it. */

/**
 * Tracks already in the list keep their places and new ones join at the end, so saving or
 * searching never moves a track the user is working through.
 */
export function keepPlaces(previous: readonly number[], next: readonly number[]): number[] {
  const fresh = new Set(next);
  const kept = previous.filter((id) => fresh.has(id));
  const known = new Set(kept);
  return [...kept, ...next.filter((id) => !known.has(id))];
}

/**
 * The track to open after a reload: the same one if it is still listed, otherwise the one that
 * took its place, so the user carries on from where they were rather than from the top.
 */
export function nextActive(
  previous: readonly number[],
  next: readonly number[],
  active: number | null,
): number | null {
  if (!next.length) return null;
  if (active == null) return next[0] ?? null;
  if (next.includes(active)) return active;
  const position = previous.indexOf(active);
  return next[Math.min(Math.max(position, 0), next.length - 1)] ?? null;
}

/** The rows a window of the list needs, plus the open track when it is outside the window. */
export function rowsToFetch(
  order: readonly number[],
  start: number,
  end: number,
  active: number | null,
): number[] {
  const wanted = order.slice(start, end);
  if (active != null && !wanted.includes(active) && order.includes(active)) wanted.push(active);
  return wanted;
}
