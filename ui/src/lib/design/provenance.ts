export type ProvenanceSource = 'file' | 'provider' | 'model' | 'user';

/** Where a shown value came from. Whether an estimate counts as low is decided by the core. */
export type Origin =
  | { kind: 'measured' }
  | { kind: 'estimated'; source: string; version?: string; confidence: number; low: boolean }
  | { kind: 'confirmed' };

export function originFromSource(
  source: ProvenanceSource,
  provider?: string,
  confidence?: number,
): Origin {
  if (source === 'file') return { kind: 'measured' };
  if (source === 'user') return { kind: 'confirmed' };
  const pct = confidence ?? 0.9;
  return {
    kind: 'estimated',
    source: provider ?? source,
    confidence: pct,
    low: pct < 0.6,
  };
}

export function describe(origin: Origin): string | null {
  switch (origin.kind) {
    case 'measured':
      return null;
    case 'estimated': {
      const pct = Math.round(origin.confidence * 100);
      if (origin.version) {
        return `Estimated by ${origin.source} ${origin.version}, ${pct}% confident`;
      }
      return `Proposed by ${origin.source} · ${pct}% match`;
    }
    case 'confirmed':
      return 'Confirmed by you';
  }
}
