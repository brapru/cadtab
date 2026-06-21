# Contributing to cadtab

## Prerequisites

- **Rust** (stable, edition 2024 — 1.85+) with the `wasm32-unknown-unknown` target:
  `rustup target add wasm32-unknown-unknown`
- **Node.js** 20+ and **npm**
- **[`just`](https://github.com/casey/just)** — the task runner (`brew install just`)
- **[`wasm-pack`](https://rustwasm.github.io/wasm-pack/)** — builds the web/wasm package
- Platform Tauri prerequisites — see <https://tauri.app/start/prerequisites/>

First-time setup:

```sh
just install      # installs frontend dependencies into app/
```

## Layout

```
Cargo.toml            workspace root
crates/cadtab-core/   pure pipeline: source text -> render tree (no UI/IO)
crates/cadtab-wasm/   wasm-bindgen bindings for the browser build
src-tauri/            Tauri 2 desktop shell
app/                  Svelte 5 + Vite frontend
```

## Running the app

All commands run from the repo root.

```sh
just dev      # desktop app (Tauri shell + Vite dev server)
just web      # browser-only frontend (Vite dev server on :5173)
```

`just dev` must be launched from the repo root so the Tauri CLI can find
`src-tauri/`; the CLI binary lives in `app/node_modules`.

## Quality gate (Definition of Done)

Every change must pass the full gate locally and in CI before it is considered
done. One command runs everything:

```sh
just check
```

It is the sum of two halves:

| | Command | Runs |
|---|---|---|
| **Rust** | `just check-rust` | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` |
| **Frontend** | `just check-ts` | `prettier --check`, `eslint`, `svelte-check`, `vitest run` |

Auto-format both languages with:

```sh
just fmt
```

New behavior ships with tests (unit / snapshot / component as fits). CI runs the
same `just check`, so a green local gate means a green CI.
