# Audiomaster — tareas comunes.
# `just` es opcional: cada receta equivale a los comandos de cargo/npm.

default:
    @just --list

# Instala dependencias de Node (Rust se gestiona con rust-toolchain.toml).
bootstrap:
    npm install

# Formato y lints.
fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

lint: fmt-check clippy
    npm run lint
    npm run format:check

# Tests.
test:
    cargo nextest run --workspace
    npm run test

test-rust:
    cargo test --workspace

# Verificación completa previa a un PR.
check: fmt-check clippy test-rust
    npm run typecheck
    npm run lint
    npm run format:check
    npm run test

# Desarrollo y empaquetado.
dev:
    npm run tauri dev

build:
    npm run tauri build

# Spikes.
spike-a *ARGS:
    cargo run --release -p infra-audio --example spike_a -- {{ARGS}}

spike-c ROWS="50000":
    cargo run --release -p infra-db --example spike_c -- {{ROWS}}
