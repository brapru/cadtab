# cadtab developer tasks. Run `just` to list them.

# Show available recipes.
default:
    @just --list

# Run the full quality gate (Rust + frontend).
check: check-rust check-ts

# Rust gate: format, lint, test.
check-rust:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test

# Frontend gate: format, lint, types, tests.
check-ts:
    npm --prefix app run format:check
    npm --prefix app run lint
    npm --prefix app run check
    npm --prefix app run test

# Coverage gate (90% lines on logic; not part of `check`, run separately/in CI).
cov: cov-rust cov-ts

# Rust line coverage for the pure core, gated at 90% lines. The thin wasm/tauri
# wrappers are glue (exercised by CI builds), so they are not measured here.
cov-rust:
    cargo llvm-cov -p cadtab-core --summary-only --fail-under-lines 90

# Frontend coverage via vitest (v8); gated at 90% lines (glue excluded in config).
cov-ts:
    npm --prefix app run test:coverage

# Auto-format Rust and frontend sources.
fmt:
    cargo fmt
    npm --prefix app run format

# Build the wasm package consumed by the web frontend.
wasm:
    wasm-pack build crates/cadtab-wasm --target web --out-dir ../../app/src/wasm-gen

# Install frontend dependencies and build the wasm package.
install:
    npm --prefix app install
    just wasm

# Run the desktop app (Tauri shell + Vite), from the repo root.
dev:
    app/node_modules/.bin/tauri dev

# Run the web app (browser-only Vite dev server).
web: wasm
    npm --prefix app run dev
