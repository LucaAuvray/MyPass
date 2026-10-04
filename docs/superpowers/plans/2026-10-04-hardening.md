# Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MyPass actually has the protections a password manager claims. That means:
- a strict CSP on the desktop and the PWA;
- no third-party calls besides HIBP;
- the master password is wiped when the vault locks;
- clippy is clean;
- production vaults are readable only by their service.

**Architecture:** Each surface gets its own CSP:
- **Desktop:** the CSP lives in `tauri.conf.json`.
- **PWA:** the server's existing static middleware sets the CSP and two extra headers on every response.

The rest of the hardening:
- **Fonts:** bundled with `@fontsource-variable`, replacing Google Fonts.
- **Unused plugins:** `fs` and `shell` are removed.
- **Master password:** `DbState` keeps it as `Zeroizing<String>`, and the new `DbState::lock_vault` drops it, together with the keyfile and the decrypted vault.

**Tech Stack:**
- Tauri v2: CSP and capabilities.
- Rust: `zeroize`, and axum middleware on the server.
- Vite and React 19 with `@fontsource-variable`.
- systemd on the container.

**Spec:** `docs/superpowers/specs/2026-10-04-hardening-design.md`

## Global Constraints

- **Desktop CSP** (`app.security.csp`, object form), exactly:
  - `default-src 'self'`;
  - `script-src 'self'`;
  - `style-src 'self' 'unsafe-inline'`;
  - `img-src 'self' data:`;
  - `font-src 'self'`;
  - `connect-src ipc: http://ipc.localhost https://api.pwnedpasswords.com`;
  - `object-src 'none'`, `base-uri 'none'`, `form-action 'none'`, `frame-ancestors 'none'`.
  - Only two evidence-driven additions are allowed: `"dangerousDisableAssetCspModification": ["style-src"]` and `https://ipc.localhost` in `connect-src`.
- **PWA CSP header**, exactly: `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' https://api.pwnedpasswords.com; worker-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`
- **PWA extra headers:**
  - `X-Content-Type-Options: nosniff`;
  - `Referrer-Policy: no-referrer`.
- **Fonts:** the only new dependencies are `@fontsource-variable/inter`, `@fontsource-variable/dm-sans` and `@fontsource-variable/jetbrains-mono`, upright only (no italic files).
- **No IPC change:** no command is added or removed, and JSON shapes are untouched.
- **Kept:** the `tauri-plugin-dialog` crate (used from Rust) and `opener` (it opens entry links in the browser).
- **Test data only:** never run anything against the production server or a real vault. E2E uses temp `APPDATA` / `MYPASS_DATA_DIR` and the identifier `com.mypass.e2e`.
- **Cargo in Git Bash** needs `export PATH="$HOME/.cargo/bin:$PATH"`. After cargo or tauri runs, run `git checkout -- src-tauri/Cargo.toml src-tauri/gen/schemas` if they only show EOL changes.
- **Container work** (Task 6 only, after Luca's go):
  - read before changing anything;
  - make a `.bak-2026-10-04` copy before editing;
  - afterwards run `systemctl status mypass-server`, `journalctl -u mypass-server` and `curl http://127.0.0.1:8787/api/health`, and show the output.
- **Commits** are in English and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Toasts under Tauri's CSP.** Tauri adds a nonce to `style-src`, which voids `'unsafe-inline'`, so the `<style>` that `sonner` injects can be blocked. A copy must still show a styled toast. If the console reports a `style-src` violation, add `dangerousDisableAssetCspModification: ["style-src"]`. Owner: E2E step in Task 4.
2. **IPC origin on Windows with `useHttpsScheme`.** Unlocking must work with no `connect-src` violation. Add `https://ipc.localhost` only if the console names it. Owner: E2E step in Task 4.
3. **Lock, then unlock again** (auto-lock or the lock button), then edit and sync. Saves and syncs must use the fresh password, and nothing of the old one may remain after the lock. Owners: unit test in Task 2, E2E step in Task 4.
4. **PWA reloaded with the server stopped.** The service worker serves the app, and the bundled fonts and the wasm load under the CSP: no `worker-src`, `font-src` or `script-src` violation, and the unlock screen uses the app's fonts. Owner: E2E step in Task 5.
5. **Clicking an entry's URL on the desktop.** It must send exactly one open request through `plugin:opener|open_url`: not zero, not two (today `shell` intercepts the click too). Owner: E2E step in Task 4.

---

### Task 1: Clippy clean on the four crates

**Files:**
- Modify:
  - `crates/mypass-core/src/generator.rs:127`: use `config.word_count.clamp(3, 20)`.
  - `crates/mypass-core/src/keys.rs:143`: unused `salt` in `test_keyfile_affects_key`. Derive both keys with `derive_composite_key_with_salt(password, …, &kdf, &salt)`. Today each call draws its own salt, so the assertion holds even if the keyfile were ignored.
  - `crates/mypass-core/src/xml.rs:424`: compare against the `&str` (`"2026-01-01"`), not an owned `String`.
  - `crates/mypass-core/src/xml.rs:5`: empty line after the doc comment.
  - `src-tauri/src/commands/database.rs:310,314`: `.map(count_entries)` and `.map(count_groups)`.
  - `src-tauri/src/native_messaging.rs:23`: empty line after the doc comment.
  - `src-tauri/src/security/nacl.rs:7`: empty line after the doc comment.
  - `src-tauri/src/security/nacl.rs:97`: `nonce` instead of `&*nonce`.
  - `src-tauri/src/ssh/agent.rs:243` (test helper): `DbState { is_open: true, keepass_file: Some(kf), ..Default::default() }`.
  - `src-tauri/tests/kdbx_integration_test.rs:3`: drop the unused `std::path::PathBuf` import.

For an empty line after a doc comment: if the `///` or `//!` block documents the item below, delete the blank line. If it is a file header, turn it into `//!`.

**Interfaces:** none (behavior unchanged).

- [ ] **Step 1: Watch it fail**

Run: `for d in crates/mypass-core crates/mypass-wasm src-tauri server; do (cd $d && cargo clippy --all-targets -- -D warnings 2>&1 | tail -1); done`
Expected: `crates/mypass-core` and `src-tauri` end with `error: could not compile` (warnings promoted); `mypass-wasm` and `server` finish.

- [ ] **Step 2: Fix the 11 sites above**

- [ ] **Step 3: Clippy and tests green**

Run: the Step 1 loop, then `(cd crates/mypass-core && cargo test) && (cd src-tauri && cargo test)`
Expected: four `Finished` lines with no warning; all tests pass (core 86, src-tauri 32+7).

- [ ] **Step 4: Commit**

```bash
git add crates/mypass-core src-tauri/src src-tauri/tests
git commit -m "chore: clippy clean with -D warnings on core and desktop"
```

---

### Task 2: Master password in `Zeroizing`, wiped on lock

**Files:**
- Modify: `src-tauri/src/commands/database.rs` (`DbState`, `Default`, `save`, `load_into_state`, `create_database`, `lock_database`, tests)
- Modify: `src-tauri/src/commands/sync.rs:240-245`

**Interfaces:**
- Produces:
  - `DbState.master_password: Option<zeroize::Zeroizing<String>>`, which replaces `password_hash`.
  - `DbState.keyfile_data: Option<zeroize::Zeroizing<Vec<u8>>>`.
  - `impl DbState { pub fn lock_vault(&mut self) }`. It sets `master_password`, `keyfile_data` and `keepass_file` to `None`, and `is_open` to `false`.

- [ ] **Step 1: Write the failing test** (in `database.rs` `mod tests`)

```rust
#[test]
fn lock_vault_forgets_password_keyfile_and_vault() {
    let mut db = DbState {
        is_open: true,
        keepass_file: Some(KeePassFile::new("t")),
        master_password: Some(zeroize::Zeroizing::new("pw".to_string())),
        keyfile_data: Some(zeroize::Zeroizing::new(vec![1, 2, 3])),
        ..Default::default()
    };
    db.lock_vault();
    assert!(!db.is_open);
    assert!(db.keepass_file.is_none());
    assert!(db.master_password.is_none());
    assert!(db.keyfile_data.is_none());
}
```

Also switch `test_save_persists_custom_data` to `master_password: Some(zeroize::Zeroizing::new(password.to_string()))`.

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test lock_vault`
Expected: compile error: no field `master_password` on `DbState`.

- [ ] **Step 3: Implement**

- Change the field types as listed under Interfaces.
- `load_into_state` and `create_database` wrap the password and the keyfile in `Zeroizing::new(...)`.
- `save()` passes `self.master_password.as_deref()` (error `"No password cached"` when `None`). This drops the `String::from_utf8` round trip.
- `sync.rs` clones `db.master_password` as a `Zeroizing<String>` (error `"mot de passe non disponible"` when `None`) and passes `&password` to the reader. It clones `keyfile_data` the same way, with `.as_deref()` at the call site.
- `lock_database` calls `clear_session_approvals()` and then `db.lock_vault()`.
- The doc comment of `lock_vault` lists what it wipes, followed by:
  `// ponytail: the decrypted entries (KeePassFile strings) are dropped, not zeroed — derive Zeroize on the xml types if memory dumps after lock matter.`
- Remove the old comments "Cached for save operations", "Simplified — use hash in production" and "Zero out sensitive data".

- [ ] **Step 4: Run tests and clippy**

Run: `cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all pass (now 33 lib tests); no warning. Also, `grep -rn password_hash src-tauri/src` prints nothing.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/database.rs src-tauri/src/commands/sync.rs
git commit -m "fix(desktop): the master password and keyfile are zeroized and wiped on lock"
```

---

### Task 3: Security headers on everything the server serves the PWA

**Files:**
- Modify: `server/src/lib.rs`. Rename `static_cache_policy` to `static_headers`, and add `const PWA_CSP: &str` with the exact string from Global Constraints. The middleware keeps its cache branches, then inserts `CONTENT_SECURITY_POLICY`, `X_CONTENT_TYPE_OPTIONS` (`nosniff`) and `REFERRER_POLICY` (`no-referrer`) on every response. Update its doc comment.
- Test: `server/tests/static_files.rs`

**Interfaces:** none (HTTP headers only; still not mounted without `MYPASS_STATIC_DIR`).

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn every_response_carries_security_headers() {
    // same tmp/web/index.html/assets/app.js setup as cache_headers_no_cache_html_immutable_assets
    const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' https://api.pwnedpasswords.com; worker-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
    for uri in ["/", "/sync", "/assets/app.js", "/api/health", "/api/vault"] {
        let res = app.clone().oneshot(request(uri)).await.unwrap();
        let h = res.headers();
        assert_eq!(h["content-security-policy"], CSP, "{uri}");
        assert_eq!(h["x-content-type-options"], "nosniff", "{uri}");
        assert_eq!(h["referrer-policy"], "no-referrer", "{uri}");
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd server && cargo test every_response_carries_security_headers`
Expected: FAIL, panic on the missing `content-security-policy` header for `/`.

- [ ] **Step 3: Implement as described under Files**

- [ ] **Step 4: Run the server suite and clippy**

Run: `cd server && cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all pass (`static_files` gains one test); no warning.

- [ ] **Step 5: Commit**

```bash
git add server/src/lib.rs server/tests/static_files.rs
git commit -m "feat(server): CSP, nosniff and no-referrer on the PWA and the API"
```

---

### Task 4: Desktop CSP, bundled fonts, fewer plugins

**Files:**
- Modify:
  - `src-tauri/tauri.conf.json`: `app.security.csp` becomes the object from Global Constraints.
  - `src-tauri/Cargo.toml`: drop `tauri-plugin-fs` and `tauri-plugin-shell`.
  - `src-tauri/src/lib.rs`: drop their `.plugin(...)` lines.
  - `src-tauri/capabilities/default.json`: permissions become exactly `["core:default", "clipboard-manager:allow-write-text", "clipboard-manager:allow-clear", "opener:default"]`.
  - `index.html`: delete the two `preconnect` links and the Google Fonts stylesheet.
  - `src/main.tsx`: import `@fontsource-variable/inter/opsz.css`, `@fontsource-variable/dm-sans/opsz.css` and `@fontsource-variable/jetbrains-mono`. Inter and DM Sans were loaded with their opsz axis, so they keep it.
  - `src/index.css:14-16`: font families become the names those CSS files declare (expected `"Inter Variable"`, `"DM Sans Variable"`, `"JetBrains Mono Variable"`; check in `node_modules`), with the same fallbacks.
  - `package.json` and `package-lock.json`, via the commands below.

**Interfaces:**
- Consumes: Task 2 (`lock_vault`), for the lock/unlock E2E step.

- [ ] **Step 1: Dependencies**

Run: `npm uninstall @tauri-apps/plugin-fs @tauri-apps/plugin-shell @tauri-apps/plugin-dialog && npm install @fontsource-variable/inter @fontsource-variable/dm-sans @fontsource-variable/jetbrains-mono`
Expected: `package.json` lists the three fontsource packages and none of the three removed plugins.

- [ ] **Step 2: Apply the file changes above**

- [ ] **Step 3: Static checks**

Run: `(cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings) && npm run lint && npm run build && grep -c "fonts.g" dist/index.html`
Expected:
- tests and lint green, build OK;
- `grep` prints `0`;
- `dist/assets` contains `.woff2` files.

- [ ] **Step 4: Release build under the test identifier**

Run: `npm run tauri build -- --no-bundle --config '{"identifier":"com.mypass.e2e"}'`
Expected: `src-tauri/target/release/mypass.exe` is built.

- [ ] **Step 5: Desktop E2E**

Set up:
- local sync server: `MYPASS_DATA_DIR` temp, `MYPASS_BIND=127.0.0.1:18787`;
- start `mypass.exe` with a temp `APPDATA` and `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`;
- drive it with the CDP helper;
- collect console and `securitypolicyviolation` events from a reload onward.

Check:
1. Create a throwaway vault and unlock it, with no `connect-src` violation (RF 2).
2. Copy a password: the toast shows, styled, with no `style-src` violation (RF 1).
3. Run HIBP on a password: a result shows.
4. Fonts: `performance.getEntriesByType("resource")` has no `fonts.googleapis.com` or `fonts.gstatic.com`, and `document.fonts` holds `Inter Variable` with status `loaded`.
5. Entry URL click: wrap `__TAURI_INTERNALS__.invoke` so it records, and swallows, any `plugin:*open*` call, then click the entry's URL. Exactly one `plugin:opener|open_url` call is recorded (RF 5).
6. Configure sync to the local server, then sync: OK.
7. Lock and unlock again, edit an entry, sync again: both OK (RF 3).
8. Over the whole run, zero CSP violations.

If item 1 or 2 shows a violation, apply only the matching allowed addition from Global Constraints, then rebuild and rerun.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/capabilities/default.json index.html src/main.tsx src/index.css package.json package-lock.json
git commit -m "feat(desktop): strict CSP, bundled fonts, no fs/shell plugins"
```

---

### Task 5: PWA under CSP, then docs

**Files:**
- Modify: `docs/superpowers/specs/2026-10-02-roadmap-design.md`. Add a sub-project 6 "Résultat (2026-10-04)" paragraph: what shipped, the evidence-driven CSP choices, and a note that the CLAUDE.md row about `/opt/mypass-src/server/` becomes stale and goes to sub-project 8. Set the tracking row to `| 6 | Durcissement | [x] | [x] | [x] 2026-10-04 |`.

**Interfaces:**
- Consumes: Task 3 (headers), Task 4 (fonts).

- [ ] **Step 1: Full suites**

Run: `for d in crates/mypass-core crates/mypass-wasm src-tauri server; do (cd $d && cargo test && cargo clippy --all-targets -- -D warnings) || break; done && npm run lint && npm run build:web && npm run smoke:wasm`
Expected: all green, and no clippy warning.

- [ ] **Step 2: PWA E2E on a local server** (`MYPASS_STATIC_DIR=../dist`, temp `MYPASS_DATA_DIR`, `127.0.0.1:18787`)

Before testing, unregister any old service worker and clear the caches. Then check:
1. `fetch("/", {method: "HEAD"})` shows the three headers.
2. Create and unlock a throwaway vault: the wasm loads.
3. Sync works.
4. HIBP returns a result.
5. A copy shows its toast.
6. `navigator.serviceWorker.controller` is set after a reload.
7. Stop the server and reload: the unlock screen still renders with `Inter Variable` loaded, with no `worker-src`, `font-src` or `script-src` violation (RF 4).
8. Over the whole run, zero CSP violations.

- [ ] **Step 3: Docs, then commit**

```bash
git add docs/superpowers/specs/2026-10-02-roadmap-design.md
git commit -m "docs: roadmap — sub-project 6 delivered"
```

---

### Task 6: Production (only after Luca's explicit go to merge, push and deploy)

**Files:** none in the repo. All the work happens on the container `root@100.64.46.117`.

- [ ] **Step 1: Server binary**
  - Sync `server/` into `/opt/mypass-src/server-build`, then run `cargo build --release` there.
  - Back up `/usr/local/bin/mypass-server` as `/root/mypass-server.bak-2026-10-04`, replace it, and restart the service.
  - Verify: `status`, `journalctl` and health show OK, and `curl -sI http://127.0.0.1:8787/` shows the CSP, `nosniff` and `no-referrer`.

- [ ] **Step 2: Vault permissions**
  - Add the drop-in `/etc/systemd/system/mypass-server.service.d/umask.conf` with `[Service]` and `UMask=0077`, then run `daemon-reload` and restart.
  - Set the existing files: `chmod 600 /var/lib/mypass/vaults/* /var/lib/mypass/token.hash*` and `chmod 700 /var/lib/mypass/vaults`.
  - Verify:
    - `systemctl show -p UMask mypass-server` prints `UMask=0077`;
    - `ls -la` shows `600` and `700`;
    - health returns 200.

- [ ] **Step 3: Cleanup.** Run `rm -rf /opt/mypass-src/server` and `rm '/opt/mypass-src/server-build/$null'`, then `ls /opt/mypass-src` to confirm.

- [ ] **Step 4: PWA and MSI.** Same procedure as sub-project 4:
  - `npm run build:web`, then a tar using a `/c/...` path;
  - on the server, back up to `/opt/mypass-web.bak-2026-10-04-sp6` and extract with `--no-same-owner`;
  - then `npm run tauri build`, and back up the old MSI as `/root/MyPass_0.1.0_x64_en-US.msi.bak-2026-10-04-sp6`;
  - verify over the tailnet:
    - the index loads the new bundle;
    - the response headers are present;
    - the downloaded MSI's sha matches the local one.

- [ ] **Step 5: Real-life check (Luca).** Luca reinstalls the MSI on both PCs, checks the browser extension, and syncs once. Then confirm that the new `vault.v<N>.kdbx` is `600`.
