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
