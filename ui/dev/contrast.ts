/** A colour token's value in each theme, as six-digit hex. */
export interface Tone {
  light: string;
  dark: string;
}

export type Palette = Record<string, Tone>;

/** Reads every `--color-name: light-dark(#light, #dark);` in the first rule of the tokens file. */
export function paletteFrom(tokensCss: string): Palette {
  const base = tokensCss.slice(0, tokensCss.indexOf('}'));
  const palette: Palette = {};
  for (const [, name, light, dark] of base.matchAll(
    /--color-([\w-]+):\s*light-dark\(\s*(#[0-9a-f]{6})\s*,\s*(#[0-9a-f]{6})\s*\)/gi,
  )) {
    if (name && light && dark) palette[name] = { light, dark };
  }
  return palette;
}

function luminance(hex: string): number {
  const channel = (start: number) => {
    const value = Number.parseInt(hex.slice(start, start + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

/** The WCAG 2 contrast ratio between two colours, from 1 to 21. */
export function contrastRatio(a: string, b: string): number {
  const [lighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (lighter + 0.05) / (darker + 0.05);
}

export const BACKGROUNDS = ['background', 'surface', 'surface-2', 'raised'];

/** What is drawn on the backgrounds, and the lowest ratio WCAG AA allows it. */
export const FOREGROUNDS: { name: string; atLeast: number; use: string }[] = [
  { name: 'text', atLeast: 4.5, use: 'Text' },
  { name: 'text-2', atLeast: 4.5, use: 'Text' },
  { name: 'muted', atLeast: 4.5, use: 'Text' },
  { name: 'accent', atLeast: 3, use: 'Playhead, selection, focus ring' },
  { name: 'border-control', atLeast: 3, use: 'Edge of a button or a field' },
];

export interface Pair {
  theme: keyof Tone;
  foreground: string;
  background: string;
  ratio: number;
  atLeast: number;
}

/** Every pair the interface draws: each foreground on each background, inverted pills, and text on the accent. */
export function pairs(palette: Palette): Pair[] {
  const tone = (name: string, theme: keyof Tone) => {
    const found = palette[name];
    if (!found) throw new Error(`The tokens file has no --color-${name}`);
    return found[theme];
  };
  const all: Pair[] = [];
  for (const theme of ['dark', 'light'] as const) {
    for (const { name, atLeast } of FOREGROUNDS) {
      for (const background of BACKGROUNDS) {
        const ratio = contrastRatio(tone(name, theme), tone(background, theme));
        all.push({ theme, foreground: name, background, ratio, atLeast });
      }
    }
    all.push({
      theme,
      foreground: 'on-accent',
      background: 'accent',
      ratio: contrastRatio(tone('on-accent', theme), tone('accent', theme)),
      atLeast: 4.5,
    });
    all.push({
      theme,
      foreground: 'background',
      background: 'text',
      ratio: contrastRatio(tone('background', theme), tone('text', theme)),
      atLeast: 4.5,
    });
    all.push({
      theme,
      foreground: 'surface',
      background: 'text',
      ratio: contrastRatio(tone('surface', theme), tone('text', theme)),
      atLeast: 4.5,
    });
  }
  return all;
}

/** The contrast table as it appears in docs/design/contrast.md. */
export function contrastTable(palette: Palette): string {
  const lines: string[] = [];
  for (const theme of ['dark', 'light'] as const) {
    const inTheme = pairs(palette).filter((pair) => pair.theme === theme);
    lines.push(`### ${theme === 'dark' ? 'Dark' : 'Light'}`, '');
    lines.push(`| On | ${BACKGROUNDS.join(' | ')} | Needs |`);
    lines.push(`| --- | ${BACKGROUNDS.map(() => '---').join(' | ')} | --- |`);
    for (const { name, atLeast } of FOREGROUNDS) {
      const ratios = BACKGROUNDS.map((background) => {
        const pair = inTheme.find((p) => p.foreground === name && p.background === background);
        return pair ? pair.ratio.toFixed(2) : '';
      });
      lines.push(`| ${name} | ${ratios.join(' | ')} | ${atLeast} |`);
    }
    const onAccent = inTheme.find((pair) => pair.foreground === 'on-accent');
    const bgOnText = inTheme.find(
      (pair) => pair.foreground === 'background' && pair.background === 'text',
    );
    const surfaceOnText = inTheme.find(
      (pair) => pair.foreground === 'surface' && pair.background === 'text',
    );
    lines.push(
      '',
      `on-accent on accent: ${onAccent?.ratio.toFixed(2)}, needs 4.5.`,
      `background on text: ${bgOnText?.ratio.toFixed(2)}, needs 4.5.`,
      `surface on text: ${surfaceOnText?.ratio.toFixed(2)}, needs 4.5.`,
      '',
    );
  }
  return lines.join('\n').trimEnd();
}
