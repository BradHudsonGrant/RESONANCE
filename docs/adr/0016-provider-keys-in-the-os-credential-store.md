# 0016: Provider keys in the OS credential store

Date: 18 November 2026. Status: accepted.

## Context

Identification needs keys: an AcoustID application key, a Discogs personal access token, and a token for each paid recogniser the user signs up to. [architecture.md](../architecture.md#dependencies-and-licences) keeps them out of the repo, logs and the database, and [product.md](../product.md#identify-and-tag) has the user enter them in Settings. The app runs on Windows, macOS and Linux, so each key has to land in that platform's own store.

## Decision

- Keys go through `keyring-core` 1.0.1 with one store crate per platform: `windows-native-keyring-store` 1.1.0 (Windows Credential Manager), `apple-native-keyring-store` 1.0.2 with the `keychain` feature (the macOS login keychain) and `zbus-secret-service-keyring-store` 1.0.1 (the Secret Service on Linux). Each is gated to its target, so a build carries only its own.
- Every key is filed under the service `com.resonance.dj`, with the provider as the account: `acoustid`, `discogs`, `audd`.
- `crates/identify` owns the `CredentialStore` seam. `OsCredentialStore` is the app's; `MemoryCredentialStore` is used by tests and by end-to-end runs, which set `RESONANCE_MEMORY_CREDENTIALS=1` so a test key never reaches a real store and CI needs no keyring daemon.
- The OS store opens on first use and is retried after a failure, so the app starts on a machine with no Secret Service and the Providers section says why a save failed.
- Three commands cross the IPC boundary: `provider_keys`, `set_provider_key` and `clear_provider_key`. A key only travels inward. Every reply carries `{ provider, configured, paid }` and nothing else.
- Store errors are reduced to four kinds before they leave the crate. `keyring-core`'s `BadEncoding` and `BadDataFormat` carry the stored bytes, so their contents are never formatted.
- `crates/core` trims what was pasted and rejects an empty value, a line break or anything over 512 characters, without repeating the value in the message.

## Why

`keyring` 3.6.3, the long-standing single crate, has had no release since July 2025. Its maintainers split it in 2026: `keyring` 4 is now a command-line sample, and they point library users to `keyring-core` and the store crates, released between May and October 2026. All are MIT OR Apache-2.0 and pass `cargo deny`.

The Secret Service store built on `zbus` is pure Rust, so a Linux build needs no `libdbus` headers. It uses the `rt-async-io-crypto-rust` feature to stay off a second async runtime and off OpenSSL.

## What follows from it

- Windows Credential Manager shows the keys as generic credentials, where a user can inspect or remove them outside the app.
- A key saved on one machine does not follow the library to another.
- Paid recogniser rows stay folded until one has a key. A new provider adds a variant to `ProviderKey` in `crates/core` and a row in `ProviderKeysSettings.svelte`.

## What would change it

A platform store that loses keys between sessions, or a provider whose credentials are more than one value, such as a host, key and secret.
