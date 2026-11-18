# 0017: Identification providers over ureq

Date: 18 November 2026. Status: accepted.

## Context

Identification works down six steps ([product.md](../product.md#identify-and-tag)). Steps 3, 4 and 6 ask AcoustID, MusicBrainz, Discogs and Bandcamp over HTTPS. Each service limits how often it can be asked, MusicBrainz wants to know who is asking, and a second run over the same files should not ask again. Jobs run on plain worker threads, and the core has no async runtime outside the dev socket.

## Decision

- HTTP goes through `ureq` 3.5.2 (MIT OR Apache-2.0), a blocking client, with `rustls` on `ring` and the operating system's certificate store through `rustls-platform-verifier`. Default features are off, so `webpki-roots`, whose CDLA-Permissive-2.0 licence is outside [ADR 0005](0005-licence-policy.md), is not in the tree.
- `cargo deny` checks the desktop targets Resonance ships for. `rustls-platform-verifier` bundles root certificates only on `wasm32`, which no build targets.
- `crates/identify` holds the `MetadataProvider` seam. Each provider declares what it answers, the key it needs and its rate limit: AcoustID 3 a second, MusicBrainz 1 a second, Discogs 60 a minute, Bandcamp 1 a second. The pipeline gives each provider its own limiter, a bucket of one token, so no window of a second ever holds more requests than the limit.
- Every request carries `User-Agent: Resonance/<version> ( https://github.com/BradHudsonGrant/RESONANCE )`.
- Answers are cached in `provider_cache`, keyed by provider and a BLAKE3 hash of the request without its key, for 30 days. A 404 is cached too.
- Results go to `track_identification`, one row per track. A track whose steps all ran is marked online and is not asked again. A step that failed or lacked its key leaves it to be tried on the next run, which costs nothing for the answers already cached.
- Bandcamp is only read for a page the user pastes, and only the structured data the page publishes.
- The rules that rank candidates live in `crates/core/src/identify.rs`, with no I/O.

## Why

`reqwest` is the other common choice. It runs on `tokio` and `hyper`, which would add an async stack to every job thread for four slow, rate-limited services. `ureq` is maintained, with commits as recent as 12 November 2026, and keeps the tree small.

`webpki-roots` would also work, but it ships Mozilla's root store inside the binary under a licence ADR 0005 does not list, and it goes stale between releases. The OS store is what the browser on the same machine trusts.

## What follows from it

- A machine whose certificate store is broken or intercepted by a proxy fails the same way its browser would.
- A new provider is one file under `crates/identify/src/providers/` and one line in `Pipeline::with_free_providers`.
- `cargo test -p resonance-identify --test live -- --ignored` checks the real MusicBrainz and Discogs answers still read as the fixtures assume.

## What would change it

A provider that needs streaming or many requests at once, or the core gaining an async runtime for another reason.
