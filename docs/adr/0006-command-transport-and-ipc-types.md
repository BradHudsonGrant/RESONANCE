# 0006: Command transport and IPC types

Date: 12 November 2026. Status: accepted as the default.

## Context

The UI is built in an ordinary browser against the core running as a local process, and the packaged app calls the same commands over Tauri IPC. The types on both sides have to come from one place. The local process must not be reachable by other pages open in the same browser.

## Decision

- Commands are one `Request` enum and one `Response` enum in the `api` crate, handled by `Api::handle`. A transport parses a request, calls that function and serialises what comes back.
- TypeScript types are generated from the Rust types with `ts-rs` (12.0.1, MIT) into `ui/src/lib/ipc/bindings/`. The tests regenerate them, the files are committed, and `just verify` fails if they are stale.
- In development the transport is a WebSocket on loopback, on a port the OS picks. `resonance serve` makes a random 256-bit token at start and refuses any upgrade request that does not carry it as a bearer credential.
- `resonance serve --connection-file <path>` writes the port and token to a file. The dev server reads it and proxies the page's WebSocket to the core, adding the credential.
- Commands and replies are JSON text frames. Binary frames are kept for real-time data.

## Why

`tauri-specta` generates a client that calls Tauri's `invoke`, which a browser page does not have. `ts-rs` only generates types, so the same types serve both transports. It also has a stable release, where `specta` and `tauri-specta` are still release candidates (2.0.0-rc.28).

A browser cannot set an `Authorization` header on a WebSocket. Requiring one means a page on any other site cannot open a connection to the core even if it guesses the port. The token never reaches the page's JavaScript, because the dev server adds it.

A port from the OS avoids clashes, and the connection file means nothing has to agree on a number in advance. A restart of the core makes a new token and the proxy picks it up on the next connection.

## What follows from it

- The thin client in the UI is typed by the generated `Request` and `Response` unions. Adding a command is a change in one Rust file and a regenerated binding.
- Ids are 32-bit so they are plain numbers in TypeScript.
- User-facing error messages are made once, in `ApiError`, on the Rust side.
- The Tauri shell calls `Api::handle` directly and does not start the socket.

## What would change it

A command set large enough that one enum becomes awkward to read would split it by area, keeping the single entry point.
