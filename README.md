# MyPass 🔒

Self-hosted password manager. One encrypted **KDBX4** file (KeePass-compatible) is the source of
truth, shared by:

- a **Windows desktop app** (Tauri v2: React + Rust),
- a **PWA** for phones (the same Rust core compiled to WebAssembly),
- a small **sync server** (Axum) that only ever stores encrypted blobs.

## Features

- 🔐 **KDBX4 vault**: AES-256-GCM or ChaCha20-Poly1305, Argon2 key derivation, opens in KeePassXC
- 🗂 **Item types**: logins, identity cards, payment cards, documents, SSH keys
- ⏱ **TOTP 2FA**: codes with countdown and copy; 2FA key or `otpauth://` link in the entry form; SHA1/SHA256/SHA512
- 📁 **Folders**: tree, filtering, create/rename/delete (content moves up), move entries
- 🔄 **Sync**: desktop and PWA push to the server; concurrent edits are merged (last write wins per entry, deletions kept as tombstones); the server keeps a versioned history
- 🎲 **Password generator**: CSPRNG passwords and passphrases with strength meter
- 🛡 **Security dashboard**: weak, reused and old passwords; breach check with HIBP k-anonymity (only a SHA-1 prefix leaves the device)
- 📥 **Import**: MyPass JSON (lossless) or CSV read by its header row (Google, Apple, KeePassXC, Bitwarden…), with duplicate review
- 📤 **Export**: MyPass JSON or CSV
- 🌐 **Browser fill** (Chrome, Edge): an adapted KeePassXC-Browser extension talks to the desktop app over Native Messaging (NaCl box)
- 🔑 **SSH agent** (Windows): serves the vault's SSH keys on the OpenSSH agent pipe, with an approval prompt
- ⬆️ **Auto-update**: the desktop app offers new signed `.msi` releases at startup
- ⏳ **Auto-lock** after idle time (default 15 min, configurable); clipboard cleared after 30 s
- 🔍 **⌘K / Ctrl+K search**, dark/light theme, English and French

## Repository layout

```
├── src/                       React 19 UI (shared by desktop and PWA)
│   └── lib/tauri.ts           every backend call: Tauri IPC, web (wasm + server) or in-memory mock
├── src-tauri/                 desktop backend (Tauri commands, SSH agent, native messaging, updater)
├── crates/mypass-core/        KDBX read/write, crypto, TOTP, generator, vault operations, merge
├── crates/mypass-wasm/        wasm-bindgen bindings of mypass-core for the PWA
├── server/                    sync server: /api/vault (ETag/If-Match), version history, PWA and /download hosting
├── keepassxc-browser/keepassxc-browser/   the browser extension (adapted KeePassXC-Browser)
├── register-nhm.ps1           registers the native messaging host for Chrome and Edge
├── scripts/release.mjs        signed .msi + updater manifest
└── docs/superpowers/, plans/  design specs and implementation plans
```

## Development

### Prerequisites (Windows)

- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/), workload "Desktop development with C++"
- [Rust](https://rustup.rs) 1.85+ (stable MSVC toolchain) and `rustup target add wasm32-unknown-unknown`
- Node.js 24+ (npm 11; `npm install` also fetches `wasm-pack`, allowed by `allowScripts` in `package.json`)

### Run

```bash
npm install
npm run build:wasm     # once, and after any change in crates/
npm run tauri dev      # desktop app with hot reload
npm run dev:web        # PWA in the browser (wasm vault + a real sync server)
npm run dev            # UI only, in-memory mock backend (no crypto, no KDBX)
```

### Check

```bash
cargo test --manifest-path crates/mypass-core/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path server/Cargo.toml
npm run lint && npm run format:check && npm run build
node src/lib/dedup.check.ts && node src/lib/groups.check.ts
```

## Self-hosting

### Sync server

The server is a single binary. It speaks plain HTTP: put it behind HTTPS (the PWA needs it),
for example with `tailscale serve`.

```bash
# on a Linux host, with Rust installed
cd server && cargo build --release
install -m 755 target/release/mypass-server /usr/local/bin/
useradd --system --home /var/lib/mypass mypass
install -d -o mypass -g mypass -m 700 /var/lib/mypass
install -d /opt/mypass-web /opt/mypass-downloads
```

`/etc/systemd/system/mypass-server.service`:

```ini
[Unit]
Description=MyPass sync server
After=network-online.target
Wants=network-online.target

[Service]
User=mypass
Group=mypass
Environment=MYPASS_DATA_DIR=/var/lib/mypass
Environment=MYPASS_BIND=127.0.0.1:8787
Environment=MYPASS_STATIC_DIR=/opt/mypass-web
Environment=MYPASS_DOWNLOAD_DIR=/opt/mypass-downloads
ExecStart=/usr/local/bin/mypass-server
Restart=on-failure
UMask=0077
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/mypass
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```bash
systemctl daemon-reload && systemctl enable --now mypass-server
journalctl -u mypass-server     # the access token is printed here, once, on first start
tailscale serve --bg 8787       # HTTPS on the tailnet
curl http://127.0.0.1:8787/api/health
```

The token is never stored in clear (only its Argon2 hash, in `token.hash`). Lost it? Delete
`/var/lib/mypass/token.hash` and restart: a new one is printed. Each client asks for the server
URL and this token.

### PWA

Build it on the PC (the server needs neither Node nor wasm-pack), then copy `dist/` into
`MYPASS_STATIC_DIR`:

```bash
npm run build:web
```

Open the server URL on the phone and add it to the home screen.

### Desktop app and auto-update

The updater checks the endpoint and public key set under `plugins.updater` in
`src-tauri/tauri.conf.json`. To publish your own builds:

1. Create a signing key once: `npx tauri signer generate -w ~/.tauri/mypass.key` (empty
   password), then put the content of `~/.tauri/mypass.key.pub` in `plugins.updater.pubkey` and
   `https://<server>/download/latest.json` in `plugins.updater.endpoints`. Never commit the
   private key.
2. For each release, bump `version` in `src-tauri/Cargo.toml` and run `npm run release`: it builds
   the signed `.msi` and `latest.json` in `src-tauri/target/release/bundle/msi/`.
3. Copy the `.msi` into `MYPASS_DOWNLOAD_DIR`, then `latest.json` last.

First install: download `https://<server>/download/MyPass_<version>_x64_en-US.msi`. Later versions
are offered by the app itself at startup.

### Browser extension (Chrome, Edge)

1. In the desktop app: **Settings → Enable browser integration**.
2. Once, from the repository folder: `powershell -ExecutionPolicy Bypass -File register-nhm.ps1`
   (writes `HKCU` keys only, no admin rights).
3. `chrome://extensions` → Developer mode → **Load unpacked** → `keepassxc-browser/keepassxc-browser/`.
4. In the extension, connect to MyPass while the vault is unlocked.

## Security

- The master password is never stored; it and the key file are zeroized when the vault locks.
- The server sees only encrypted KDBX blobs and a hashed access token.
- Strict Content Security Policy on the desktop app and on the PWA; no third-party requests.
- Browser traffic is end-to-end encrypted (NaCl box), and the extension's ID is pinned.
