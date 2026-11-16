# 0014: Open-source release under the MIT licence

Date: 16 November 2026. Status: accepted. Updates [ADR 0005](0005-licence-policy.md).

## Context

[ADR 0005](0005-licence-policy.md) kept the dependency tree permissive while the repository was private so the project could later open its source or ship commercially. Opening the repository on GitHub requires choosing the licence that ADR 0005 deferred.

Resonance is built and maintained solo. Opening the repository lets other DJs and developers read the code, build it from source, inspect how files are handled and learn from the architecture, without turning the project into a community-managed codebase.

## Decision

- Licence Resonance under the [MIT Licence](../../LICENSE) (`Copyright (c) 2026 Bradley Hudson-Grant`), and set `license = "MIT"` across the Cargo workspace and `ui/package.json`.
- Keep every dependency and model rule in [ADR 0005](0005-licence-policy.md): permissive runtime dependencies only, enforced by `cargo-deny` in CI and bundled in `app/THIRD-PARTY-NOTICES.txt`, with non-commercial model weights remaining opt-in downloads.
- Develop the project solo and do not accept external pull requests. State that plainly in `README.md` and `CONTRIBUTING.md`, protect `master` with a GitHub branch ruleset, and restrict repository interactions to collaborators.

## Why

MIT is short, standard across the Rust and Svelte ecosystems, and already on the `cargo-deny` allow list. Anyone can read, build, fork or adapt the code under clear terms, while the author keeps full freedom over future releases without needing contributor licence agreements.

Declaring upfront that external pull requests are not accepted avoids wasting anyone's time writing patches against a fast-moving roadmap where each milestone is designed and verified as a single coherent sequence.

## What follows from it

- `LICENSE` sits at the repository root and GitHub marks the repository as MIT-licensed.
- Every workspace crate and `ui/package.json` declare `MIT`.
- `master` requires a pull request, passing `verify` CI checks and linear history under a GitHub ruleset, matching the local `pre-push` hook.

## What would change it

If external contributions are welcomed after `1.0`, `CONTRIBUTING.md` and the repository interaction settings can open up without changing the licence.
