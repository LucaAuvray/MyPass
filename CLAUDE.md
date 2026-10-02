# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
npm install            # install frontend deps
npm run dev             # Vite dev server only (port 1420, browser mock backend — see below)
npm run tauri dev       # full app with Rust backend + hot reload (requires Rust toolchain)
npm run build           # tsc -b && vite build (frontend only)
npm run tauri build     # native binary build
npm run lint            # eslint . --ext ts,tsx --max-warnings 0
npm run format          # prettier --write
npm run format:check
npm run build:wasm      # wasm-pack build de crates/mypass-wasm (requis : wasm-pack installé)
npm run dev:web         # Vite dev en mode web (coffre wasm + sync serveur, pas de mock)
npm run build:web       # build de l'app web (déployée sur le serveur de sync)
npm run smoke:wasm      # smoke test Node du module wasm
```

`npm run build` exécute `build:wasm` d'abord — **wasm-pack doit être installé** sinon le build frontend échoue.

Rust backend (`src-tauri/`):
```bash
cd src-tauri
cargo build
cargo test                    # run all Rust unit tests
cargo test crypto::tests      # run a single module's tests, e.g. kdbx::crypto
cargo clippy
```

There is no frontend test runner configured (no vitest/jest in package.json) — only Rust has unit tests, embedded `#[cfg(test)]` modules in `kdbx/crypto.rs`, `kdbx/keys.rs`, and `security/nacl.rs`.

On this machine, `npm run tauri` prepends `C:\Users\Utilisateur\.cargo\bin` to PATH (see the `tauri` script in package.json) — Rust must be installed there for Tauri CLI commands to find `cargo`.

## Architecture

MyPass is a Tauri v2 desktop password manager: React/TypeScript frontend + Rust backend, communicating over Tauri's IPC (`invoke`), with a KDBX4 (KeePass-compatible) database file as the source of truth — there is no SQL database.

### Frontend/backend split

- `src/` — React 19 UI. State: Zustand stores (`src/stores/{app,database,entries}Store.ts`) for client state, TanStack Query for async/server-style state. All calls into Rust go through `src/lib/tauri.ts`.
- `src-tauri/src/` — Rust backend:
  - `commands/` — one file per domain (`database`, `entries`, `groups`, `generator`, `totp`, `import_export`, `passkeys`, `browser`), each exposing `#[tauri::command]` functions. Every command must be registered in the `tauri::generate_handler![]` list in `lib.rs` — adding a command in a file alone does nothing until it's added there.
  - `kdbx/` — KDBX4 file format implementation from scratch: `reader.rs`/`writer.rs` (file structure), `crypto.rs` (AES-GCM/ChaCha20-Poly1305 encryption), `keys.rs` (Argon2 key derivation), `xml.rs` (the inner XML payload).
  - `security/` — `nacl.rs` (NaCl box crypto for the browser protocol), `hibp.rs` (Have I Been Pwned k-anonymity checks), `zxcvbn.rs` (password strength scoring).
  - `native_messaging.rs` — implements the Chrome Native Messaging Host protocol standalone (not a Tauri plugin).

### Critical: browser dev mode uses a mock backend

`src/lib/tauri.ts` checks `"__TAURI__" in window` at runtime. Outside a real Tauri window (i.e. `npm run dev` in a plain browser), it swaps in an in-memory mock implementation of every command instead of calling Rust. This means **`npm run dev` alone cannot exercise real KDBX/crypto/Rust behavior** — any change touching actual database/crypto logic must be verified with `npm run tauri dev`, not just the Vite dev server.

### Web mode & sync (self-hosted)

- `crates/mypass-core/` — the KDBX/crypto/ops core, extracted from `src-tauri` (which re-exports it as `kdbx`). Compiles to wasm32.
- `crates/mypass-wasm/` — wasm-bindgen bindings over the core (JSON strings in/out, `thread_local` session mirroring the desktop DbState). Built by `npm run build:wasm` into `crates/mypass-wasm/pkg` (aliased as `@wasm`).
- `server/` — standalone Axum sync server (GET/PUT `/api/vault`, ETag/If-Match/409, Bearer auth, versioned encrypted blobs). Also serves the web app build when `MYPASS_STATIC_DIR` is set. Deployed on a Proxmox LXC, reached over Tailscale.
- `src/lib/web.ts` — third backend of `getInvoke()` in `tauri.ts` (native Tauri → web → browser mock), selected by `vite --mode web`: the vault lives in the wasm session, every mutation is pushed to the server (dirty flag, serialized push chain, 409 → merge → retry).
- Desktop sync lives in `src-tauri/src/commands/sync.rs` + `crates/mypass-core/src/merge.rs` (LWW merge, tombstones) — the web engine in `web.ts` mirrors it and must keep JSON shape parity with the Tauri IPC.

### Native Messaging bridge (browser extension integration)

MyPass talks to a browser extension using the same protocol as KeePassXC-Browser (NaCl box encryption, Native Messaging), so it can reuse that existing extension instead of building a new one:

- `src-tauri/src/main.rs` — the same binary doubles as the Native Messaging Host: launched normally it runs the Tauri app; launched with `--native-messaging` it runs `run_native_messaging_host()` (stdin/stdout JSON protocol) instead of opening a window.
- `register-nhm.ps1` / `mypass-nhm.bat` — Windows-only scripts that register this binary as a Native Messaging Host in the Chrome/Edge/Firefox registry/config so those browsers know to launch `mypass.exe --native-messaging`.
- `commands/browser.rs` — Tauri commands the frontend UI uses to check/toggle browser-integration status.
- `keepassxc/` and `keepassxc-browser/` at the repo root are full vendored clones of the upstream KeePassXC desktop app and browser extension (each has its own `.git`) — kept as protocol/behavior reference, not built or imported by this project. `extension/` is this project's own (currently minimal) Manifest V3 extension.

### Import/Export

`commands/import_export.rs` and `src/lib/dedup.ts` implement import from CSV, 1Password (1PUX), Bitwarden, Google, Apple, and Proton Pass, plus a dedup pass when merging into an existing database. Export supports CSV, JSON, XML, HTML.

### Design/planning docs

`plans/` contains architecture and feature-design docs written before/during implementation (e.g. `mypass-architecture-plan.md`, `browser-integration-plan.md`, `dedup-feature-plan.md`) — check these for the reasoning behind non-obvious decisions (e.g. why KeePassXC-Browser is reused rather than rebuilt).

## Frontend conventions

- Path alias `@/` → `src/` (configured in both `vite.config.ts` and `tsconfig.json`).
- UI components are shadcn/ui (`style: base-nova`, Base UI primitives, Tailwind v4, `src/components/ui/`) — add new primitives via the shadcn CLI rather than hand-rolling, to stay consistent with `components.json`.
- i18n via `react-i18next`, translations in `src/i18n/` (English + French) — user-facing strings should go through it rather than being hardcoded.
