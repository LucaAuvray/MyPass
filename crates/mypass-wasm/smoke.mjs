// Smoke test runtime du wasm dans Node (mêmes APIs que le navigateur :
// Date.now, crypto.getRandomValues). Prouve que l'horloge js et le RNG
// fonctionnent à l'exécution — la compilation wasm32 seule ne le garantit pas.
// Usage : npm run build:wasm && npm run smoke:wasm
import { readFileSync } from "node:fs";
import init, {
  create_vault, open_vault, close_vault, is_unlocked, save_vault,
  create_entry, get_entries, merge_remote,
  generate_totp_secret, generate_totp_code, generate_password,
  export_entries, parse_import, import_entries,
} from "./pkg/mypass_wasm.js";

await init({ module_or_path: readFileSync(new URL("./pkg/mypass_wasm_bg.wasm", import.meta.url)) });

let failed = false;
const check = (cond, msg) => {
  console.log(`${cond ? "ok  " : "FAIL"} - ${msg}`);
  if (!cond) failed = true;
};

// Cycle de vie + horloge vivante (LMT doit être l'heure réelle, pas 1970)
const bytes = create_vault("Smoke", "s3cret");
open_vault(bytes, "s3cret", undefined);
check(is_unlocked(), "coffre créé et ouvert");

const entry = JSON.parse(
  create_entry(JSON.stringify({ title: "Site", username: "luca", password: "p4ss" })),
);
check(entry.modified > "2026", `horloge js vivante (modified=${entry.modified})`);

// Save → reopen
const saved = save_vault();
close_vault();
open_vault(saved, "s3cret", undefined);
check(JSON.parse(get_entries(null)).length === 1, "entrée persistée après save/reopen");

// Fusion no-op avec soi-même
const outcome = JSON.parse(merge_remote(saved));
check(outcome.changed === false, "fusion no-op avec sa propre sauvegarde");

// Outils (RNG + horloge TOTP)
const code = JSON.parse(generate_totp_code(generate_totp_secret(), undefined, undefined, undefined));
check(/^\d{6}$/.test(code.code), `code TOTP généré (${code.code})`);
check(
  generate_password(JSON.stringify({ length: 20, uppercase: true, lowercase: true, digits: true, symbols: true })).length === 20,
  "générateur de mot de passe",
);

// Import / export : export JSON → parse_import → import_entries (doublons voulus ici)
const before = JSON.parse(get_entries(null)).length;
const parsed = JSON.parse(parse_import(export_entries("json", "[]")));
check(parsed.length === before, `export JSON relu (${parsed.length} entrée(s))`);
const imported = JSON.parse(import_entries(JSON.stringify(parsed)));
check(imported.imported === before && JSON.parse(get_entries(null)).length === 2 * before, "import_entries écrit les entrées");
let csvError = "";
try { parse_import("foo,bar\n1,2"); } catch (e) { csvError = String(e); }
check(csvError.includes("CSV_NO_HEADER"), `CSV sans en-tête reconnu refusé (${csvError})`);

close_vault();
if (failed) { console.error("SMOKE FAILED"); process.exit(1); }
console.log("SMOKE OK");
