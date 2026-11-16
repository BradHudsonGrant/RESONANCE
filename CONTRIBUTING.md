# Contributing

Resonance is built and maintained solo, and external pull requests are not accepted ([ADR 0014](docs/adr/0014-open-source-under-mit.md)). This page records the rules the repository follows so anyone reading or building the code can see how changes are made and verified. Everything below is a requirement.

## Code

- Names come from the domain: `Deck`, `BeatGrid`, `Asset`. No `utils`, `helpers`, `manager` or `service` buckets.
- A comment explains why something is the way it is, and most code needs none. No comments that repeat the code, no banner comments, no "Step 1", no emoji, no commented-out code, no TODO without an issue number.
- No abstraction on spec. A trait with one implementation is allowed only at a real seam: providers, analysers, adapters, input devices. No option nobody sets.
- Nothing pretends to work. If something is simulated, its type and the UI say so.
- A file does one thing. Past about 400 lines, ask whether it is doing two.
- Handle an error where something can be done about it. Otherwise pass it up with context.
- Tests read as a description of behaviour. No tests that only check a mock was called. No giant snapshots.
- Logs are structured and sparse and contain no secrets.
- `rustfmt`, `clippy -D warnings`, `eslint`, `prettier`, `svelte-check` and `typos` must pass. No exceptions without a comment saying why.

## Tests

- `core` and `metrics`: unit and property tests (time-mapping round trips, key maths, duration maths).
- Audio: fixtures generated inside the tests. Click tracks at a known tempo, tones, known offsets, known loudness. The expected answers are exact. No copyrighted audio in git.
  The `fixtures` crate makes them and writes them as WAV or AIFF. Add it as a dev-dependency.
- Real music: `just fixtures-local <music folder>` copies about 50 tracks across formats into `fixtures-local/`, with a `manifest.json` of where each came from and its hash. It only reads the originals. Git ignores the folder, so tests that need it run on the machine that has it and nowhere else.
- Scale: `just synthetic-library` writes 100,000 made-up tracks with plausible tags into `target/dev/synthetic`, for measuring the library at sizes no real folder here has.
- File operations: integration tests in temporary folders, covering undo and a crash halfway through an operation.
- Database: a migration test from every earlier version.
- Integrations: small synthetic libraries committed as fixtures.
- UI: component tests with Vitest, a short Playwright suite for the main flows against a real core (`just e2e`, also part of `just verify`), and one smoke test against the real app before each release. The suite runs in the installed Edge on its own port, core and library, so it never touches a dev server or `just serve`.
- Budgets: `just bench` measures every budget in `docs/architecture.md` and writes the result to `docs/benchmarks/budgets-vX.Y.Z.md`, with the machine and versions. It opens the desktop app and an Edge window and plays audio for about four minutes, so it is run by hand at the end of a milestone and not in `just verify`. [ADR 0011](docs/adr/0011-bench-in-the-page-and-the-cli.md) says how each figure is taken.

## Docs

The repo has a README (what it is, a screenshot, status, install, build, licence), `docs/architecture.md`, `docs/adr/NNNN-title.md`, `docs/design/`, `docs/roadmap.md`, `CONTRIBUTING.md` and a generated `CHANGELOG.md`.

Write plainly: say what the thing does and why. Avoid "robust", "seamless", "comprehensive", "powerful", lists of three for rhythm, bold used as decoration, and closing lines that sum up what was just said.

## Public repository safety

- Test audio is generated from maths in `crates/fixtures`. Never commit copyrighted audio, embedded album artwork, recorded mixes, or screenshots showing real tracks or cover art; screenshots in `docs/` and pull requests use `just synthetic-library` or generated fixtures only.
- Never commit SQLite databases (`*.db`, `*.sqlite`), real `rekordbox`, `Engine DJ`, `Serato` or `Traktor` library files, raw HTTP captures containing headers or tokens, or paths naming a local user profile or checkout folder.
- Never commit third-party encryption keys (such as `rekordbox` `master.db` SQLCipher keys), firmware blobs, or controller mappings copied from copyleft projects. `just repo-safety` (part of `just verify`) checks every tracked path and file in git.

## Done means

A change is done when it is merged through a PR, CI is green, `just verify` passes on the laptop, the behaviour has a test, the docs that describe it are updated, and nothing in it is a stub.

`just verify` is one command that runs format, lint, licence check, tests, build and the attribution check. Use `just` as the task runner, or `cargo xtask` if you would rather not add a tool. The name of the command matters less than there being exactly one.

## Identity and attribution

Commits, PRs and releases carry the author's name and nothing else. No co-author trailers and no generator footers.

Set the identity in the repo's local config from the GitHub profile:

```
gh api user --jq '{id, login, name}'
git config user.name  "<name>"
git config user.email "<id>+<login>@users.noreply.github.com"
```

A local `commit-msg` hook rejects a message that carries a co-author trailer or a generator footer. `just verify` searches the branch's commit messages for the same strings and fails if it finds one. Check PR bodies before posting them.

## Repository

Public repository on GitHub (`BradHudsonGrant/RESONANCE`), default branch `master`, licensed under [MIT](LICENSE) ([ADR 0005](docs/adr/0005-licence-policy.md), [ADR 0014](docs/adr/0014-open-source-under-mit.md)).

Repository settings: squash merge on, merge commits and rebase merges off, delete branch on merge, wiki off, issues on, and topics `dj`, `music-library`, `audio-analysis`, `tauri`, `rust`, `svelte`.

`master` is protected by a GitHub branch ruleset (pull request required, `verify` status check required, linear history, no force pushes, no deletion) and by a local `.git/hooks/pre-push` hook that refuses direct pushes to `refs/heads/master`. Changes reach `master` only through `gh pr merge --squash`. Repository interaction limits restrict issues and pull requests to collaborators while solo development continues.

## Branches, commits and PRs

Branches: `feature/`, `fix/`, `docs/`, `chore/`, `refactor/`, `test/`, `perf/`, then area and subject in lowercase with hyphens. For example `feature/library-folder-scan` or `fix/tags-id3-year-frame`.

Commits follow Conventional Commits: `feat(library): scan folders with subfolder toggles`. Imperative mood, subject under 72 characters, a body when the reason isn't obvious, no trailers.

Each PR is one logical change, usually under 400 changed lines not counting lock files and generated code. Its title is the squash commit subject. Its body follows the template: what, why, how to test, screenshots for UI changes, notes. It links an issue with `Closes #n`, sits in a milestone and has labels. Read your own diff before merging. Merge when CI is green.

Every PR has an issue. Issue templates: feature, bug, task. Labels: `type:*` and `area:*`. Milestones are named by version, such as `v0.2.0`.

The history should read like one careful developer working through a roadmap. Don't land a milestone as one PR. Don't split one change into filler commits either. Leave timestamps alone.

## Versions and releases

SemVer, 0.x until 1.0. One minor version per milestone, patch versions for fixes. A script keeps the version in step across the Cargo workspace, `tauri.conf.json` and `package.json`.

To release: open `chore/release-vX.Y.Z`, set the version with `just version X.Y.Z`, write `CHANGELOG.md` with `just changelog vX.Y.Z`, merge the PR, tag `vX.Y.Z` on `master`. `just verify` fails if the version files disagree, and the release workflow fails if the tag does not match them. The tag triggers a workflow that builds the Windows installer and drafts a GitHub Release. Edit the notes for a human reader, then publish. Mark 0.x releases as pre-releases.

## CI

- On every PR, on `ubuntu-latest` with caching: format, lint, `cargo-deny`, tests for the platform-neutral crates, UI lint, type check and tests.
- On tags, on `windows-latest`: full build, tests, installer.
- Before every PR, on the laptop: `just verify`. The laptop is the real target, so this is the check that counts.
