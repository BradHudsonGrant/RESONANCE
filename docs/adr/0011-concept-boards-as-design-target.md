# 0011: Concept boards as the design target

Date: 13 November 2026. Status: accepted. Replaces the "mood only" rule in [product.md](../product.md#decisions-already-made) and the first layout sketch in [ux.md](../design/ux.md).

## Context

Until now the brief treated `concept-dark.png` as mood only (monochrome palette, one-pixel waveforms, 3D dot waves, one orange accent) while its own ASCII sketch and 4 px / 6 px corner radii took priority. M0 built the first shell to that sketch: a 52 px icon rail, a 44 px header with a right-hand command box, a definition-list inspector and a 48 px footer.

Two concept boards sit in `handoff/`: `concept-dark.png` (a complete library screen with top bar, expanded sidebar, command and slash bar, table, inspector, A to B transition dock and player bar) and `concept are and design 2.png` (brand marks, palette, typography, micro elements, component samples and dark and light desktop screens). Rebuilding the shell and existing views to the boards in M1 means M2 and later milestones build each screen once against its final layout.

## Decision

Both concept boards become the design target. `concept-dark.png` leads for layout, proportions, corner radii and density; `concept are and design 2.png` supplies the brand marks, micro elements, component samples and light-mode counterpart.

- **Corner radii and spacing.** Radii increase from the brief's 4 px and 6 px to `6px` on buttons, inputs, chips, data tags and selected rows (`--radius-control`), `8px` on the command bar, artwork thumbnails, deck cards, dropdowns and stat cards (`--radius-panel`), `10px` on outer framed panels, confirm sheets and set cards (`--radius-lg`), and `9999px` (`--radius-pill`) on filter pills and circular transport controls. Spacing stays on the 4 px scale.
- **Shell layout.** A 40 px top window bar holds the seven-bar mark and wordmark. A collapsible left sidebar (220 px open by default, 52 px collapsed) lists full space names and counts. The centre column opens with a full-width search and command bar and a strip of slash commands above the workspace. The inspector sits on the right (320 px). The bottom player area combines the A to B transition dock above the 48 px player bar.
- **Spaces and naming.** The five sidebar spaces keep one plain word each: `Library`, `Prep` (replacing `Inbox`, taking its name from `concept-dark.png`'s `Preparation`), `Sets`, `Mix` and `Replay` (replacing `Review`). `Activity` and `Settings` sit at the bottom of the sidebar. `Intelligence`, `History` and `Analysis` have no pages of their own; their facts appear on the tracks and sessions they describe. Consumer-streaming pages on the second board (`Listen`, `Explore`, `For You`, `Profile`) are left out.
- **No collection cards above the library table.** `Library` stays a dense 30 px virtualised table so the tracks keep the vertical space. Dot-wave cards move to `Sets` (M10), and collections list under `Library` in the sidebar once they exist (M2).
- **No gradient fills.** Panels, bars, buttons, table rows and charts stay flat, separated by `1px` borders. Visual depth comes only from the 3D dot wave ([wave.md](../design/wave.md)), where monochrome dots fade with distance in the shader and the accent marks only a focus bracket.
- **Dot-wave fallback thumbnails.** Every track without embedded cover art receives a deterministic still dot-wave thumbnail (`WaveThumb`) seeded from its own data, so rows, deck cards and the inspector always show art without stock imagery. Embedded artwork replaces the fallback once tags bring it in M3.
- **WCAG AA contrast stays mandatory.** The palette tokens already match both boards and stay enforced by `ui/dev/contrast.ts`. Because `--color-accent` falls below 4.5:1 on light surfaces and raised dark surfaces, active labels and badges keep `--color-text` or `--color-on-accent` for text and confine `--color-accent` to non-text marks, borders and solid accent fills ([contrast.md](../design/contrast.md)).

## Why

`concept-dark.png` gives the scannable hierarchy and density needed for browsing thousands of tracks, inspecting audio facts and cueing transitions, without giving up the 30 px table or the monochrome palette already verified in M0. Naming the spaces `Library`, `Prep`, `Sets`, `Mix` and `Replay` preserves the single-home rule from [product.md](../product.md) and keeps the UI focused on local library management, analysis, mixing and review.

Keeping CSS gradients out of panels and charts avoids banding on desktop displays and keeps contrast predictable across both themes, while reusing the WebGL2 dot-wave component that already runs under a 1 ms UI-thread frame budget.

## What follows from it

- Issue [#50](https://github.com/BradHudsonGrant/RESONANCE/issues/50) updates `ui/src/lib/design/tokens.css`, adjusts existing primitives and adds the micro elements, segmented tabs, player controls, `WaveThumb` and `WaveCard` to `/design.html` in both themes.
- Issue [#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51) rebuilds the app shell: top window bar, collapsible sidebar, full-width command bar with slash-command strip, inspector frame, A to B dock frame and player bar.
- Issue [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52) rebuilds the Library and Mix views using only the data that exists today, and re-measures the library table ([#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40)) and Mix view ([#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46)) budgets.
- Columns, panels and controls whose data arrives in a later milestone stay out until that milestone or say plainly what they are waiting for, never showing made-up values.
