# Contrast

Every pair of colours the interface draws, with its WCAG 2 contrast ratio. Text needs 4.5 to 1 for AA. Marks that are not text, such as the playhead, a selection or the edge of a field, need 3 to 1.

The numbers come from `ui/src/lib/design/tokens.css`. A test works them out again on every run, fails if a pair drops below its minimum, and fails if this table no longer matches the tokens. To print the table again:

```
node ui/dev/contrast-table.ts
```

## What changed from the first palette

Three tones in the first palette failed AA and were moved until they passed on all four backgrounds:

| Token | Theme | Was | Worst ratio | Now |
| --- | --- | --- | --- | --- |
| Muted | Dark | `#777777` | 3.42 on Raised | `#8C8C8C` |
| Muted | Light | `#8A8783` | 2.98 on Surface 2 | `#6C6965` |
| Text 2 | Light | `#666461` | passed at 4.91 | `#595754` |

Light Text 2 passed already. It moved because the new light Muted landed almost on top of it, and the two have to stay a visible step apart.

Two tokens are new:

- Control border is the edge of a button or a field. The ordinary border is a divider between areas and sits near 1.5 to 1 on purpose, which is too faint to be the only thing marking a control.
- On accent is the text colour on an accent background. White and the dark theme's text colour reach only 3.3 to 4.1 on the accent, so the primary button's label is near-black in both themes.

## The table

### Dark

| On | background | surface | surface-2 | raised | Needs |
| --- | --- | --- | --- | --- | --- |
| text | 17.43 | 16.31 | 14.76 | 13.55 | 4.5 |
| text-2 | 8.28 | 7.75 | 7.01 | 6.44 | 4.5 |
| muted | 5.85 | 5.48 | 4.95 | 4.55 | 4.5 |
| accent | 5.31 | 4.97 | 4.49 | 4.13 | 3 |
| border-control | 3.87 | 3.62 | 3.28 | 3.01 | 3 |

on-accent on accent: 5.31, needs 4.5.
background on text: 17.43, needs 4.5.
surface on text: 16.31, needs 4.5.

### Light

| On | background | surface | surface-2 | raised | Needs |
| --- | --- | --- | --- | --- | --- |
| text | 16.74 | 17.93 | 14.93 | 17.93 | 4.5 |
| text-2 | 6.73 | 7.20 | 6.00 | 7.20 | 4.5 |
| muted | 5.10 | 5.46 | 4.55 | 5.46 | 4.5 |
| accent | 3.82 | 4.09 | 3.41 | 4.09 | 3 |
| border-control | 3.38 | 3.62 | 3.01 | 3.62 | 3 |

on-accent on accent: 4.81, needs 4.5.
background on text: 16.74, needs 4.5.
surface on text: 17.93, needs 4.5.

## Additional pairs from the concept boards

The concept boards add inverted pills (an active filter pill, an active curve toggle pill, an active segmented tab and solid monochrome action buttons) that draw surface or background text over a `--color-text` fill, plus accent-highlighted badges and sidebar items. Every text pair keeps at least 4.5 to 1; non-text marks keep at least 3 to 1.

| Pair | Dark | Light | Needs | Where it appears |
| --- | --- | --- | --- | --- |
| `background` on `text` | 17.43 | 16.74 | 4.5 | Active filter pills (`All Tracks`), active `Energy` pill, solid monochrome `Play` button |
| `surface` on `text` | 16.31 | 17.93 | 4.5 | Active segmented tab (`All`) when placed inside a surface container |
| `text` on `surface-2` with `accent` 1 px border | 14.76 (text), 4.49 (border) | 14.93 (text), 3.41 (border) | 4.5 (text), 3 (border) | Accent-flagged data tags (such as `AI` or active model flags) |
| `accent` on `surface-2` | 4.49 | 3.41 | 3 | Non-text marks only (selection bars, icons, focus brackets); never text |
| `accent` on `raised` | 4.13 | 4.09 | 3 | Non-text marks only (`2px` selection bar on active sidebar or table row); never text |

On the brand board, the `AI` tag and the active `Listen` sidebar label draw orange text on dark or light surfaces. Because `--color-accent` sits at 4.49 on dark `surface-2`, 4.13 on dark `raised`, and between 3.41 and 4.09 across all four light backgrounds, it fails the 4.5 text threshold. Resonance keeps active labels in `--color-text` and puts `--color-accent` on the border, left selection bar, focus bracket or icon, or uses `--color-on-accent` (`#0B0B0C`) text on a solid `--color-accent` fill (5.31 dark, 4.81 light).

## More contrast and high contrast

When the system asks for more contrast, dividers take the control border's tone and Muted takes Text 2's. In Windows high contrast mode the system's colours replace the palette, and the accent follows the system highlight colour.
