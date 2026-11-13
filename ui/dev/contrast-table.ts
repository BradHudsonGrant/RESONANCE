import { readFileSync } from 'node:fs';

import { contrastTable, paletteFrom } from './contrast.ts';

const tokens = readFileSync(new URL('../src/lib/design/tokens.css', import.meta.url), 'utf8');

console.log(contrastTable(paletteFrom(tokens)));
