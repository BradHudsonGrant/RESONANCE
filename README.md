# Resonance

Resonance is a desktop app for DJs. It looks after a music library and helps the person using it get better at mixing: tagging, audio analysis, mixing with live coaching, and review of recorded mixes.

Most DJs will keep playing on their own hardware. Resonance is where they prepare, practise and review.

## Status

Early. The first milestone, M0 Foundations (v0.1.0), is in progress and there is nothing to install yet. [docs/roadmap.md](docs/roadmap.md) shows what is done and what is next.

## Docs

| File | What is in it |
| --- | --- |
| [docs/product.md](docs/product.md) | What Resonance is, the rules every feature follows, and the features |
| [docs/roadmap.md](docs/roadmap.md) | The milestones as checklists |
| [docs/architecture.md](docs/architecture.md) | The stack, the repository layout, the rules, the data model and the performance budgets |
| [docs/adr/](docs/adr/) | The decisions and the reasons for them |
| [docs/design/](docs/design/) | Structure, layout, interaction, the visual language and the wave |
| [docs/v3-notes.md](docs/v3-notes.md) | What the previous app taught us |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How work gets into the repo |

## Stack

A Rust workspace for the core, Tauri 2 for the desktop shell and Svelte 5 for the UI. Windows first.

## Licence

The code is not yet licensed for reuse. A licence comes if and when the repo becomes public.
