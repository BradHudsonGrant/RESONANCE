/** Where a shown value came from. Whether an estimate counts as low is decided by the core. */
export type Origin =
  | { kind: 'measured' }
  | { kind: 'estimated'; source: string; version: string; confidence: number; low: boolean }
  | { kind: 'confirmed' };

export function describe(origin: Origin): string | null {
  switch (origin.kind) {
    case 'measured':
      return null;
    case 'estimated':
      return `Estimated by ${origin.source} ${origin.version}, ${Math.round(origin.confidence * 100)}% confident`;
    case 'confirmed':
      return 'Confirmed by you';
  }
}
