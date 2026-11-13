# Contributing

How work gets into this repo. Everything below is a requirement.

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
- File operations: integration tests in temporary folders, covering undo and a crash halfway through an operation.
- Database: a migration test from every earlier version.
- Integrations: small synthetic libraries committed as fixtures.
- UI: component tests with Vitest, a short Playwright suite for the main flows against mocked IPC, and one smoke test against the real app before each release.
- `resonance bench` writes hardware, versions and timings to `docs/benchmarks/`.

## Docs

The repo has a README (what it is, a screenshot, status, install, build, licence), `docs/architecture.md`, `docs/adr/NNNN-title.md`, `docs/design/`, `docs/roadmap.md`, `CONTRIBUTING.md` and a generated `CHANGELOG.md`.

Write plainly: say what the thing does and why. Avoid "robust", "seamless", "comprehensive", "powerful", lists of three for rhythm, bold used as decoration, and closing lines that sum up what was just said.

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

```
gh repo create BradHudsonGrant/RESONANCE --private \
  --description "Music library manager and DJ practice tool: tagging, analysis, mixing and live coaching."
git branch -M master
```

Then set, with `gh repo edit` (check `--help`, flags change): squash merge on, merge commits and rebase merges off, delete branch on merge, wiki off, issues on, and topics `dj`, `music-library`, `audio-analysis`, `tauri`, `rust`, `svelte`.

The GitHub account is on the Free plan, which has no branch protection or rulesets for private repositories. So add `.git/hooks/pre-push` that refuses a push to `refs/heads/master`. Changes reach `master` only through `gh pr merge --squash`. If the repo goes public later, add a ruleset as well: PR required, status checks required, linear history, no force pushes.

No LICENSE file while the repo is private. The README says the code is not yet licensed for reuse. The licence is chosen when the repo goes public, and the licence policy in [docs/adr/0005-licence-policy.md](docs/adr/0005-licence-policy.md) keeps both options open.

## Branches, commits and PRs

Branches: `feature/`, `fix/`, `docs/`, `chore/`, `refactor/`, `test/`, `perf/`, then area and subject in lowercase with hyphens. For example `feature/library-folder-scan` or `fix/tags-id3-year-frame`.

Commits follow Conventional Commits: `feat(library): scan folders with subfolder toggles`. Imperative mood, subject under 72 characters, a body when the reason isn't obvious, no trailers.

Each PR is one logical change, usually under 400 changed lines not counting lock files and generated code. Its title is the squash commit subject. Its body follows the template: what, why, how to test, screenshots for UI changes, notes. It links an issue with `Closes #n`, sits in a milestone and has labels. Read your own diff before merging. Merge when CI is green.

Every PR has an issue. Issue templates: feature, bug, task. Labels: `type:*` and `area:*`. Milestones are named by version, such as `v0.2.0`.

The history should read like one careful developer working through a roadmap. Don't land a milestone as one PR. Don't split one change into filler commits either. Leave timestamps alone.

## Versions and releases

SemVer, 0.x until 1.0. One minor version per milestone, patch versions for fixes. A script keeps the version in step across the Cargo workspace, `tauri.conf.json` and `package.json`.

To release: open `chore/release-vX.Y.Z`, bump versions, generate `CHANGELOG.md` with git-cliff, merge the PR, tag `vX.Y.Z` on `master`. The tag triggers a workflow that builds the Windows installer and drafts a GitHub Release. Edit the notes for a human reader, then publish. Mark 0.x releases as pre-releases.

## CI

The Free plan includes 2,000 Actions minutes a month for private repositories, and Windows runners use them at double rate.

- On every PR, on `ubuntu-latest` with caching: format, lint, `cargo-deny`, tests for the platform-neutral crates, UI lint, type check and tests.
- On tags, on `windows-latest`: full build, tests, installer.
- Before every PR, on the laptop: `just verify`. The laptop is the real target, so this is the check that counts.
