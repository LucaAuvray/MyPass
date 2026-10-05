# Auto-Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish a desktop version once and both PCs update themselves. At startup the app finds the new version, asks, installs the signed `.msi` and comes back on the new version.

**Architecture:**
- **Updater:** `tauri-plugin-updater`, used from Rust only. A new `updater.rs` checks a static `latest.json` served under `/download/` at startup (release builds only) and asks through a native dialog.
- **Browser relay:** it exits when the app goes away, so it no longer locks `mypass.exe` during the install.
- **Release:** `npm run release` builds the signed `.msi` and writes `latest.json`. The deploy steps are written down in `CLAUDE.md`.
- **Server:** unchanged.

**Tech Stack:**
- Tauri v2.11: `tauri-plugin-updater` 2.x, and `tauri-plugin-dialog` 2.7 (already present).
- Node for the release script.
- The `/download/` route of the existing server.

**Spec:** `docs/superpowers/specs/2026-10-05-auto-update-design.md`

## Global Constraints

- **Endpoint**, exactly: `https://mypass-luca.tail7687c9.ts.net/download/latest.json`. `windows.installMode` is `"passive"`.
- **The webview gets nothing:**
  - no npm updater or process package;
  - no `updater:*` or `process:*` permission in `src-tauri/capabilities/default.json`.
- **When to check:** only in release builds (`cfg!(debug_assertions)` returns early), and only once, at startup.
- **Prompt dialog** (title `MyPass`, modal on the `main` window):
  - text: `MyPass {version} est disponible (installée : {current_version}). L'app va se fermer pour l'installer.`
  - buttons: `MessageDialogButtons::OkCancelCustom("Installer", "Plus tard")`.
- **Error dialog** (title `MyPass`, `MessageDialogKind::Error`): `La mise à jour a échoué : {error}`.
- **Quiet failures:** "up to date", offline and an unreachable server show nothing; they log with `eprintln!` (repo style).
- **Signing key:**
  - lives at `%USERPROFILE%\.tauri\mypass.key` (+ `.pub`), with no password;
  - its contents are never printed, copied into the repo or committed. Only the public key goes into `tauri.conf.json`.
- **Version:** the only source is `version` in `src-tauri/Cargo.toml`. Remove `version` from `tauri.conf.json` and from `package.json`.
- **Building:** since `createUpdaterArtifacts` is on, a plain `npm run tauri build` needs the key in the environment. Use `npm run release`, or `--no-bundle` for E2E builds.
- **Test data only:**
  - never install the E2E build;
  - never serve a real MSI with a valid signature to a local E2E app;
  - E2E uses the identifier `com.mypass.e2e`, a temp `APPDATA` and a local server on `127.0.0.1:18787`.
- **Cargo in Git Bash** needs `export PATH="$HOME/.cargo/bin:$PATH"`. After cargo or tauri runs, `git checkout -- src-tauri/gen/schemas` (and `src-tauri/Cargo.toml` when the change is EOL-only).
- **Container work** (Tasks 5 and 6 only, after Luca's go):
  - read before changing anything;
  - make a `.bak-<date>` copy before editing;
  - afterwards run `systemctl status mypass-server` and `curl http://127.0.0.1:8787/api/health`, and show the output.
- **Commits** are in English and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Chrome open with the extension connected during an update.** Its relay holds `mypass.exe`.
   - *Expected:* the MSI replaces the exe without a "restart Windows" prompt, the app comes back, and the extension fills a login afterwards.
   - *Owners:* the relay E2E in Task 4, and the real update in Task 6.
2. **UAC declined after "Installer".** The app has already exited.
   - *Expected:* nothing changes (the old version stays installed), and relaunching asks again.
   - *Owner:* Task 6, on this PC, as the first attempt.
3. **A manifest whose version is the same as, or older than, the installed one** (after a rollback, say).
   - *Expected:* no dialog. There are no downgrades.
   - *Owner:* the E2E in Task 4.
4. **The `.msi` URL returns 404** (the manifest went up first, or the file was pruned).
   - *Expected:* "Installer" shows the error dialog and the app keeps running.
   - *Owner:* the E2E in Task 4.
5. **A PC off the tailnet at startup.**
   - *Expected:* no dialog, no delay before unlocking, and the app works.
   - *Owner:* the E2E in Task 4.

---

### Task 1: Updater plugin and startup check

**Files:**
- Create: `src-tauri/src/updater.rs`
- Modify:
  - `src-tauri/Cargo.toml` (dependency);
  - `src-tauri/tauri.conf.json` (`bundle.createUpdaterArtifacts`, `plugins.updater`);
  - `src-tauri/src/lib.rs` (`mod updater;`, `.plugin(...)`, a call in `setup`).

**Interfaces:**
- Produces: `pub fn check_on_startup(app: &tauri::AppHandle)`. It returns immediately and runs the check on `tauri::async_runtime::spawn`.

- [ ] **Step 1: Generate the key pair**

Run: `npx tauri signer generate --ci -p "" -w "$USERPROFILE/.tauri/mypass.key"`

Expected: `mypass.key` and `mypass.key.pub` exist. Check only `ls -l` and `wc -c`; never `cat` the private key. Then tell Luca the path so he stores a copy in his vault.

- [ ] **Step 2: Configure the updater**

In `tauri.conf.json`:
- set `bundle.createUpdaterArtifacts: true`;
- set `plugins.updater`:
  - `pubkey`: the exact contents of `mypass.key.pub`;
  - `endpoints`: the Global Constraints URL;
  - `windows.installMode`: `"passive"`.

In `Cargo.toml`, add `tauri-plugin-updater = "2"`. In `lib.rs`:
- register `.plugin(tauri_plugin_updater::Builder::new().build())` next to the other plugins;
- call `updater::check_on_startup(app.handle())` at the end of `setup`.

- [ ] **Step 3: Implement `check_on_startup` in `src-tauri/src/updater.rs`**

The flow, top to bottom:
1. return early on `cfg!(debug_assertions)`;
2. in the spawned task, call `app.updater()?.check().await`;
3. on `Ok(None)` or `Err`, call `eprintln!` and stop;
4. on `Some(update)`, show the prompt dialog from Global Constraints with `.parent(&main_window)`, using `show(|install| …)`. The callback bool already maps the custom "Installer" label to `true`;
5. on `true`, spawn `update.download_and_install(|_, _| {}, || {}).await`. On Windows it never returns on success (the plugin calls `process::exit(0)` after starting msiexec with `AUTOLAUNCHAPP=True`). On `Err`, show the error dialog.

Copy the strings exactly as written in Global Constraints. Use a `//!` header in the repo style, saying what the module does and why it runs in release builds only.

- [ ] **Step 4: Build and lint**

Run (in `src-tauri`):
- `cargo test`;
- `cargo clippy --all-targets -- -D warnings`;
- then, at the repo root, `npm run tauri build -- --no-bundle`.

Expected: all green. `capabilities/default.json` is unchanged (`git diff --stat` shows no capability change).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json src-tauri/src/lib.rs src-tauri/src/updater.rs
git commit -m "feat(desktop): check for a signed update at startup and offer to install it"
```

### Task 2: The browser relay exits when the app goes away

**Files:**
- Modify: `src-tauri/src/native_messaging.rs`, in `run_native_messaging_proxy`, the `from_app_thread` closure.

**Interfaces:**
- Produces: no new interface. The `--native-messaging` process ends as soon as the app closes the bridge connection.

- [ ] **Step 1: Exit the process when the app side closes**

Today the main thread stays blocked in `io::copy(stdin → app)` until Chrome writes again or closes the port, so the process outlives the app and keeps `mypass.exe` locked.

Fix: after the read loop in `from_app_thread`, call `std::process::exit(0)`. Add a comment saying the reason: the installer has to be able to replace the exe, and the extension relaunches the relay on its next use.

- [ ] **Step 2: Build and lint**

Run (in `src-tauri`): `cargo test` and `cargo clippy --all-targets -- -D warnings`.

Expected: green. The behaviour itself is checked by the relay E2E in Task 4.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/native_messaging.rs
git commit -m "fix(desktop): the browser relay exits with the app so updates can replace the exe"
```

### Task 3: Release script, single version source and written procedure

**Files:**
- Create: `scripts/release.mjs`
- Modify:
  - `package.json` (add the `"release": "node scripts/release.mjs"` script, remove `version`);
  - `package-lock.json` (via `npm install --package-lock-only`);
  - `src-tauri/tauri.conf.json` (remove `version`);
  - `CLAUDE.md`.

**Interfaces:**
- Consumes: the key from Task 1.
- Produces: `npm run release` writes these files to `src-tauri/target/release/bundle/msi/`:
  - `MyPass_<v>_x64_en-US.msi`;
  - `MyPass_<v>_x64_en-US.msi.sig`;
  - `latest.json`, in this shape:

```json
{ "version": "<v>", "pub_date": "<ISO 8601 now>",
  "platforms": { "windows-x86_64": {
    "signature": "<contents of the .msi.sig>",
    "url": "https://mypass-luca.tail7687c9.ts.net/download/MyPass_<v>_x64_en-US.msi" } } }
```

- [ ] **Step 1: Single version source**

Remove `version` from `tauri.conf.json` and from `package.json`, then run `npm install --package-lock-only`.

Expected: `npm run lint` and `npm run build` still pass, and `npm run tauri build -- --no-bundle` reports the version from `Cargo.toml` (`0.1.0`).

- [ ] **Step 2: Write `scripts/release.mjs`**

1. Read `<v>` from the first `version = "…"` line of `src-tauri/Cargo.toml`.
2. Fail with a clear message if `%USERPROFILE%\.tauri\mypass.key` is missing.
3. Run `npm run tauri build` (`spawnSync`, `shell: true`, `stdio: "inherit"`), with `TAURI_SIGNING_PRIVATE_KEY` set to the key path and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` set to `""`.
4. Fail if the build fails or the `.sig` is missing.
5. Write `latest.json` (shape above) and print its path and `<v>`. Never print the key.

- [ ] **Step 3: Run it**

Run: `npm run release`.

Expected:
- the three files exist;
- `latest.json` has `version` `0.1.0`, a `url` ending in `MyPass_0.1.0_x64_en-US.msi`, and a `signature` equal to the `.sig` contents (compare with `node -e`, without printing either).

This `latest.json` and `.sig` are not deployed; Task 4 reuses the `.sig`.

- [ ] **Step 4: Write the procedure in `CLAUDE.md`**

- **Infrastructure table, `/opt/mypass-downloads/` row:** it now holds `MyPass_<version>_x64_en-US.msi` (the current one and the previous one) plus `latest.json` (the updater manifest).
- **The "Le `.msi` est construit…" line:** replace it with "built on the Windows PC with `npm run release`, never on the container; see the release section".
- **Commands block:** add `npm run release`.
- **New section `## Release desktop`** (in French, like the rest of the file):
  1. bump `version` in `src-tauri/Cargo.toml` and commit;
  2. run `npm run release`;
  3. on the container, `cp latest.json latest.json.bak-<date>`;
  4. `scp` the `.msi`, then `latest.json` as `latest.json.new`, then `mv` it over (the manifest goes last);
  5. check through the tailnet: the version in `latest.json`, and the sha256 of the downloaded `.msi` against the local one;
  6. keep the current and previous `.msi`, and delete older ones.

  Then a "Clé de signature" note:
  - path, and no password;
  - copy in Luca's vault;
  - if lost: a new key pair, then a manual reinstall on each PC.

- [ ] **Step 5: Commit**

```bash
git add scripts/release.mjs package.json package-lock.json src-tauri/tauri.conf.json CLAUDE.md
git commit -m "build: npm run release signs the msi and writes latest.json; Cargo.toml holds the version"
```

### Task 4: Local E2E of the update check

**Files:**
- Throwaway only, in the scratchpad: the E2E dir, and the UI Automation helper `uia.ps1`. Nothing is committed.

**Interfaces:**
- Consumes:
  - `check_on_startup` (Task 1) and the relay fix (Task 2);
  - the `MyPass_0.1.0_x64_en-US.msi.sig` from Task 3.

- [ ] **Step 1: Build the E2E app and start a local server**

Build:
```
npm run tauri build -- --no-bundle --config '{"identifier":"com.mypass.e2e","plugins":{"updater":{"endpoints":["http://127.0.0.1:18787/download/latest.json"],"dangerousInsecureTransportProtocol":true}}}'
```
The output is `src-tauri/target/release/mypass.exe`.

Start the server: `cargo run --release` in `server/`, with:
- `MYPASS_BIND=127.0.0.1:18787`;
- `MYPASS_DATA_DIR=<e2e>/data`;
- `MYPASS_DOWNLOAD_DIR=<e2e>/dl`.

Launch the app with `APPDATA=<e2e>/appdata`.

`uia.ps1` lists the top-level windows of a PID with their text and buttons, and clicks a button by name. Use `System.Windows.Automation`; fall back to `powershell.exe` 5.1 if `pwsh` cannot load it.

- [ ] **Step 2: Cases with no dialog**

Each case starts a fresh app and waits 15 s.

| Setup | Expected |
|---|---|
| Server stopped | No dialog. The unlock screen is usable (Review Focus 5). |
| `dl/latest.json` with `version` `0.1.0` (the current one) | No dialog (Review Focus 3). |

- [ ] **Step 3: Cases with a dialog**

Use `dl/latest.json` with `version` `9.9.9` and the real `.sig` contents as `signature`. The served file is `dl/fake.msi`, a text file. It is not an MSI, so msiexec can never start, even if the signature were wrongly accepted.

| Setup | Action | Expected |
|---|---|---|
| `url` → `…/download/fake.msi` | — | The prompt shows exactly `MyPass 9.9.9 est disponible (installée : 0.1.0). L'app va se fermer pour l'installer.` with the buttons `Installer` and `Plus tard`. |
| same | "Plus tard" | The dialog closes and the app stays usable. |
| same, relaunched | "Installer" | The error dialog starts with `La mise à jour a échoué :` and names the signature. No `msiexec` process appears, and the app is alive. |
| `url` → `…/download/missing.msi` | "Installer" | The error dialog appears and the app is alive (Review Focus 4). |

- [ ] **Step 4: The relay exits with the app**

1. With the E2E app running, check that `Get-NetTCPConnection -LocalPort 25798 -State Listen` belongs to the E2E app's PID. If Luca's installed MyPass holds the port, ask him to quit it; never kill his app.
2. Spawn `mypass.exe --native-messaging` from Node with `stdio: "pipe"`, and keep its stdin open.
3. Stop the E2E app by PID.

Expected: the relay process exits within 1 s (Review Focus 1).

- [ ] **Step 5: Clean up**

Stop the server and the app by PID, and delete `<e2e>`.

Expected: `git status` is clean, except for the EOL churn handled by Global Constraints.

### Task 5: Release 0.1.1 to production, and the manual install

Prerequisites: the branch is reviewed and merged, and **Luca has said to push and deploy**.

**Files:**
- Modify: `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock` (version `0.1.1`).

- [ ] **Step 1: Bump, release, push**

1. Set `version = "0.1.1"` in `Cargo.toml` and run `cargo check` (to update `Cargo.lock`).
2. Commit `chore: release 0.1.1`.
3. Run `npm run release`, then `git push`.

- [ ] **Step 2: Deploy** (the `CLAUDE.md` procedure)

1. Back up the July manifest: `cp -a /opt/mypass-downloads/latest.json /opt/mypass-downloads/latest.json.bak-2026-10-05`.
2. `scp` `MyPass_0.1.1_x64_en-US.msi`, then `latest.json` as `latest.json.new`, then `mv`. Files are `root:root 644`, like the existing ones.
3. The 0.1.0 `.msi` stays (it is the previous one).

Expected, through the tailnet:
- `latest.json` has `version` `0.1.1` and the same `signature` as the local one;
- the downloaded `.msi` has the same sha256 as the local one;
- `/api/health` is 200.

- [ ] **Step 3: Luca installs 0.1.1 by hand**

1. **This PC.** Expected: the registry entry `HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*` for MyPass shows `DisplayVersion` `0.1.1`, and the app shows no dialog at launch.
2. **The laptop.** He installs it, then fetches the vault from the server (typing the token and the password himself). Expected: an entry created on this PC shows up on the laptop. That closes sub-project 1.

### Task 6: Release 0.1.2 and the automatic update

**Files:**
- Modify:
  - `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock` (`0.1.2`);
  - `docs/superpowers/specs/2026-10-02-roadmap-design.md` (results of SP7 and SP1, and the table rows).

- [ ] **Step 1: Bump, release, deploy**

1. Bump to `0.1.2`, commit `chore: release 0.1.2`, run `npm run release`.
2. Deploy as in Task 5, with the backup named `latest.json.bak-2026-10-05-v0.1.1`.
3. Delete `MyPass_0.1.0_x64_en-US.msi` from `/opt/mypass-downloads` (keep 0.1.1 and 0.1.2).

Expected: the same checks as in Task 5 Step 2, now with `0.1.2`.

- [ ] **Step 2: Update this PC**

Before launching MyPass, Luca opens Chrome with the extension connected (Review Focus 1). Then he launches MyPass:
1. **First try:** the dialog appears. "Installer", then he declines the UAC prompt. Expected: the app is closed, and relaunching shows the dialog again (Review Focus 2).
2. **Second try:** "Installer", then he accepts UAC. Expected: a progress bar, then the app comes back by itself.

Then I check:
- `DisplayVersion` is `0.1.2`;
- the version of `C:\Program Files\MyPass\mypass.exe` (`(Get-Item …).VersionInfo.ProductVersion`) is `0.1.2`;
- there is no pending `mypass.exe` entry in `HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager` → `PendingFileRenameOperations`.

Luca fills a login with the extension.

- [ ] **Step 3: Update the laptop**

Same as Step 2, without the UAC-decline step. Luca confirms it restarts on 0.1.2.

- [ ] **Step 4: Roadmap and wrap-up**

1. **Roadmap:**
   - add a "Résultat (2026-10-05)" paragraph to sub-project 7 (bootstrap 0.1.1, then the 0.1.2 auto-update; anything found on the way);
   - mark the remaining item of sub-project 1 as done if the laptop got there;
   - tick both rows in the table.
2. Commit `docs: roadmap — sub-project 7 delivered`, and push after Luca's go.
3. Update the memory file `deploy-and-e2e-gotchas.md` with any new trap.
