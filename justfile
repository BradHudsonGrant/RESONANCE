set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

base := "origin/master"
attribution_pattern := "co-authored-by|generated with"

# Format check, lint, licence check, tests, build, and the attribution check
verify: fmt-check lint licences test bindings-current build ui-verify attribution
    @echo "verify passed: format, clippy, typos, licences, tests, bindings, build, ui, attribution"

fmt-check:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings
    typos

licences:
    cargo deny check licenses sources

test:
    cargo test --workspace

build:
    cargo build --workspace

# Type check, lint, format check, tests and build for the UI. Needs `npm ci --prefix ui` once.
ui-verify:
    npm --prefix ui run check
    npm --prefix ui run lint
    npm --prefix ui run test
    npm --prefix ui run build

# The UI in a browser with hot reload, talking to the core that `just serve` runs
ui:
    npm --prefix ui run dev

# The core as a local process, rebuilt and restarted when a crate changes, serving the library in the data folder
serve data_folder="target/dev/data":
    node ui/dev/core-watch.ts --data-folder "{{ data_folder }}"

# Read a music folder into the library that `just serve` serves. Nothing in the folder is changed.
scan folder data_folder="target/dev/data":
    cargo run --quiet --package resonance-cli -- --data-folder "{{ data_folder }}" scan "{{ folder }}"

# A made-up library for measuring at scale. Serve it with `just serve target/dev/synthetic`.
synthetic-library count="100000":
    cargo run --quiet --package resonance-fixtures --bin synthetic-library -- target/dev/synthetic {{ count }}

# Clean and incremental build times for the dev profile, recorded in docs/benchmarks/build-times.md
build-times:
    node ui/dev/build-times.ts

# About 50 real tracks copied from a music folder into fixtures-local/, which git ignores. The originals are only read.
fixtures-local source:
    cargo run --quiet --package resonance-fixtures -- "{{ source }}"

# The tests regenerate the TypeScript types, so any change here means the committed ones are stale
[unix]
bindings-current:
    @test -z "$(git ls-files --modified --others --exclude-standard -- ui/src/lib/ipc/bindings)"

[windows]
bindings-current:
    @if (git ls-files --modified --others --exclude-standard -- ui/src/lib/ipc/bindings) { Write-Host 'The TypeScript types in ui/src/lib/ipc/bindings are out of date. Stage the regenerated files.'; exit 1 }

# Commits carry the author's name only, so no message on the branch may hold a co-author trailer or a generator footer
[unix]
attribution:
    @test -z "$(git log {{ base }}..HEAD -i -E --grep='{{ attribution_pattern }}' --format='%h %s')"

[windows]
attribution:
    @if (git log {{ base }}..HEAD -i -E --grep='{{ attribution_pattern }}' --format='%h %s') { Write-Host 'A commit message on this branch carries a co-author trailer or a generator footer.'; exit 1 }
