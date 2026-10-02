# MyPass 🔒

> Modern, secure, open-source password manager — inspired by 1Password, powered by KeePassXC technology.

## Tech Stack

| Layer | Technology |
|---|---|
| **Runtime** | [Tauri v2](https://v2.tauri.app) — tiny, fast native binaries |
| **Frontend** | React 19 + Vite 6 + TypeScript |
| **Styling** | Tailwind CSS v4 + shadcn/ui (Base UI, Nova theme) |
| **State** | Zustand + TanStack Query v5 |
| **Backend** | Rust — crypto, KDBX, import/export, TOTP, Passkeys |
| **Protocol** | NaCl box encryption (KeePassXC-Browser compatible) |
| **Extension** | Manifest V3 browser extension (Chrome, Firefox, Edge) |
| **PWA** | Workbox service worker, offline-first |

## Features

- 🔐 **KDBX4 database** — fully compatible with KeePassXC
- 🔑 **42 Tauri commands** — Rust-powered backend for all operations
- 🎨 **Modern UI** — glass-morphism, dark/light mode, responsive
- 🔍 **⌘K search** — instant command palette
- 🎲 **Password generator** — CSPRNG passwords & passphrases with strength evaluation
- ⏱ **TOTP 2FA** — SHA1/SHA256/SHA512 support
- 🔒 **Passkeys** — FIDO2/WebAuthn credential management
- 📥 **Import** — CSV, 1Password (1PUX), Bitwarden, Google, Apple, Proton Pass
- 📤 **Export** — CSV, JSON, XML, HTML
- 🛡 **Security dashboard** — weak/reused/old password detection, HIBP k-anonymity
- ⏳ **Auto-lock** — idle activity timer
- 🌐 **Browser integration** — NaCl-encrypted Native Messaging bridge
- 🌍 **i18n** — English & French
- 📱 **PWA** — installable on mobile & desktop

## Getting Started

### Prerequisites
- Node.js 20+
- Rust 1.80+
- Tauri system dependencies ([see docs](https://v2.tauri.app/start/prerequisites/))

### Development
```bash
npm install
npm run dev        # Start Vite dev server (port 1420)
npm run tauri dev  # Start Tauri app with hot-reload
```

### Build
```bash
npm run build      # Build frontend
npm run tauri build # Build native binary
```

## Project Structure

```
mypass/
├── src/                  # React frontend
│   ├── components/       # UI components (layout, entries, groups, security, etc.)
│   ├── hooks/            # Custom hooks (useDatabase, useEntries, etc.)
│   ├── stores/           # Zustand stores (app, database, entries)
│   ├── lib/              # Utilities (tauri, crypto, password-strength)
│   ├── types/            # TypeScript types (entry, group, database, import)
│   ├── views/            # Page views (AllItems, Security, Passkeys, Browser)
│   └── i18n/             # Translations (en, fr)
├── src-tauri/            # Rust backend
│   └── src/
│       ├── commands/     # Tauri commands (database, entries, groups, etc.)
│       ├── kdbx/         # KDBX parser/writer (crypto, keys, xml, reader, writer)
│       └── security/     # NaCl protocol, HIBP, password health
├── extension/            # Browser extension (Manifest V3)
└── plans/                # Architecture documentation
```

## Security

- **Zero-knowledge**: Master password never stored
- **Memory safety**: Rust `zeroize` for sensitive data
- **Clipboard**: Auto-cleared after 30 seconds
- **Auto-lock**: After 5 minutes of inactivity
- **HIBP**: k-anonymity (only SHA-1 prefix sent)
- **Browser**: NaCl box end-to-end encryption

## License

MIT License — see [LICENSE](LICENSE) file.
