import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { contrastRatio, contrastTable, pairs, paletteFrom } from './contrast.ts';

const tokens = readFileSync(new URL('../src/lib/design/tokens.css', import.meta.url), 'utf8');
const palette = paletteFrom(tokens);

describe('contrastRatio', () => {
  it('gives 21 for black on white and 1 for a colour on itself', () => {
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 5);
    expect(contrastRatio('#e4553d', '#e4553d')).toBe(1);
  });

  it('matches the published ratio for a known pair', () => {
    expect(contrastRatio('#777777', '#ffffff')).toBeCloseTo(4.48, 2);
  });
});

describe('the colour tokens', () => {
  it('are all read from the tokens file', () => {
    expect(Object.keys(palette).sort()).toEqual([
      'accent',
      'background',
      'border',
      'border-control',
      'muted',
      'on-accent',
      'raised',
      'surface',
      'surface-2',
      'text',
      'text-2',
    ]);
  });

  it.each(pairs(palette))(
    '$theme: $foreground on $background meets WCAG AA',
    ({ ratio, atLeast }) => {
      expect(ratio).toBeGreaterThanOrEqual(atLeast);
    },
  );

  it('keep muted text a visible step below Text 2 in both themes', () => {
    for (const theme of ['dark', 'light'] as const) {
      const onBackground = (name: string) =>
        contrastRatio(palette[name]![theme], palette['background']![theme]);
      expect(onBackground('text-2') - onBackground('muted')).toBeGreaterThan(1);
    }
  });
});

describe('docs/design/contrast.md', () => {
  it('holds the table these tokens give', () => {
    const doc = readFileSync(new URL('../../docs/design/contrast.md', import.meta.url), 'utf8');

    expect(doc.replaceAll('\r\n', '\n')).toContain(contrastTable(palette));
  });
});
