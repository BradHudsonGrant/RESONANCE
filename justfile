set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

base := "origin/master"
attribution_pattern := "co-authored-by|generated with"
shell_exclusion := if os() == "windows" { "" } else { "--exclude resonance-app" }
export CARGO_HOME := justfile_directory() / "target/cargo-home"
export NPM_CONFIG_CACHE := justfile_directory() / "target/npm-cache"

# Format check, lint, licence check, tests, build, and the attribution check
verify: ui-verify fmt-check lint licences test bindings-current build notices-current version-current attribution
    @echo "verify passed: format, clippy, typos, licences, tests, bindings, build, ui, notices, version, attribution"

fmt-check:
    cargo fmt --all --check

lint:
    cargo clippy --workspace {{ shell_exclusion }} --all-targets -- -D warnings
    typos

licences:
    cargo deny check licenses sources

test:
    cargo test --workspace {{ shell_exclusion }}

build:
    cargo build --workspace {{ shell_exclusion }}

# Type check, lint, format check, tests and build for the UI. Needs `npm ci --prefix ui` once.
ui-verify:
    npm --prefix ui run check
    npm --prefix ui run lint
    npm --prefix ui run test
    npm --prefix ui run build

# The UI in a browser with hot reload, talking to the core that `just serve` runs
ui:
    npm --prefix ui run dev

# The desktop shell uses the same development library, with its webview data beside it.
app data_folder="target/dev/data":
    npm --prefix ui run build
    cargo run --package resonance-app --features custom-protocol -- --data-folder "{{ data_folder }}"

# Native scroll measurements over the library made by `just synthetic-library`.
app-bench:
    npm --prefix ui run build -- --mode benchmark
    cargo run --release --package resonance-app --features custom-protocol -- --data-folder target/dev/synthetic

# The Windows installer, with bundled assets and project-local packaging tools.
[windows]
[working-directory: 'app']
installer:
    npm --prefix ../ui run build
    node ../ui/dev/notices.ts --check
    ../ui/node_modules/.bin/tauri.cmd build --bundles nsis

# Licence texts from the exact Rust dependencies and the frontend's bundled modules.
notices:
    npm --prefix ui run build
    node ui/dev/notices.ts

notices-current:
    node ui/dev/notices.ts --check

# The core as a local process, rebuilt and restarted when a crate changes, serving the library in the data folder
serve data_folder="target/dev/data":
    node ui/dev/core-watch.ts --data-folder "{{ data_folder }}"

# Read a music folder into the library that `just serve` serves. Nothing in the folder is changed.
scan folder data_folder="target/dev/data":
    cargo run --quiet --package resonance-cli -- --data-folder "{{ data_folder }}" scan "{{ folder }}"

# A made-up library for measuring at scale. Serve it with `just serve target/dev/synthetic`.
synthetic-library count="100000":
    cargo run --quiet --package resonance-fixtures --bin synthetic-library -- target/dev/synthetic {{ count }}

# Set the version in the Cargo workspace, tauri.conf.json and package.json at once
version new:
    node ui/dev/version.ts "{{ new }}"

version-current:
    node ui/dev/version.ts

# CHANGELOG.md from the commit history. Pass the tag about to be made, such as v0.2.0, to head the newest changes with it.
changelog tag="":
    git cliff {{ if tag == "" { "" } else { "--tag " + tag } }} --output CHANGELOG.md

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
