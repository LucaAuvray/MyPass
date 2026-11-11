//! Passkeys (WebAuthn ES256) served to the browser extension through the
//! `passkeys-register` / `passkeys-get` actions, stored in the vault in
//! KeePassXC's `KPEX_PASSKEY_*` fields.

pub mod webauthn;
