// Runnable check of the pending save (background/page.js) with a stubbed browser API: `node keepassxc-browser/pending.check.mjs`
import { readFileSync } from "node:fs";
import vm from "node:vm";

const src = readFileSync(new URL("./keepassxc-browser/background/page.js", import.meta.url), "utf8");
const store = new Map();
const browser = {
    storage: {
        session: {
            get: async (k) => (store.has(k) ? { [k]: structuredClone(store.get(k)) } : {}),
            set: async (o) => { for (const [k, v] of Object.entries(o)) store.set(k, structuredClone(v)); },
            remove: async (k) => { store.delete(k); },
        },
        local: { get: async () => ({}), set: async () => {} },
    },
    // Public suffixes reject the dummy cookie, like Chrome does
    cookies: {
        set: async ({ domain }) => { if (["com", "co.uk", "uk"].includes(domain)) throw new Error("no"); return {}; },
        remove: async () => {},
    },
};
const ctx = { browser, console, URL, Date, structuredClone,
    SORT_BY_MATCHING_CREDENTIALS_SETTING: 0, SORT_BY_RELEVANT_ENTRY: 0, SORT_BY_GROUP_AND_TITLE: 0,
    BannerPosition: { TOP: 0 }, CHECK_UPDATE_NEVER: 0, ManualFill: { NONE: 0 } };
const page = vm.runInNewContext(`${src}\n;page`, ctx);

let failed = 0;
const check = (ok, msg) => { console.log(`${ok ? "ok  " : "FAIL"} - ${msg}`); if (!ok) failed++; };
const tab = (url, id = 7) => ({ id, url });

await page.setSubmitted(tab("https://www.example.com/signup"), [true, "", "GEN", "https://www.example.com", [], true]);
let p = await page.getSubmitted(tab("https://www.example.com/onboarding"));
check(p.submitted && p.generated && p.password === "GEN" && p.username === "", "generated pending readable on the same host, empty username");
check((await page.getSubmitted(tab("https://app.example.com/x"))).generated, "same base domain (subdomain) keeps it");
check(Object.keys(await page.getSubmitted(tab("https://www.example.com/x", 8))).length === 0, "another tab sees nothing");

await page.setSubmitted(tab("https://www.example.com/signup"), [true, "luca@x.fr", "GEN", "https://www.example.com", []]);
p = await page.getSubmitted(tab("https://www.example.com/"));
check(p.generated && p.username === "luca@x.fr", "submitting the same password keeps generated");

await page.setSubmitted(tab("https://www.example.com/signup"), [true, "", "OTHER", "https://www.example.com", []]);
p = await page.getSubmitted(tab("https://www.example.com/"));
check(!p.generated && p.username === "luca@x.fr", "different password drops generated, empty username keeps the earlier one");

check(Object.keys(await page.getSubmitted(tab("https://evil.co.uk/"))).length === 0, "leaving the site returns {}");
check(!store.has("pending_7"), "...and deletes the pending save");

await page.setSubmitted(tab("https://shop.example.co.uk/a"), [true, "u", "P", "https://shop.example.co.uk", [], true]);
check((await page.getSubmitted(tab("https://www.example.co.uk/b"))).generated, "co.uk base domain matches");
check(Object.keys(await page.getSubmitted(tab("https://other.co.uk/b"))).length === 0, "other co.uk site does not");

await page.setSubmitted(tab("https://www.example.com/signup"), [true, "u", "P", "https://www.example.com", [], true]);
const v = store.get("pending_7"); v.time -= 15 * 60 * 1000 + 1; store.set("pending_7", v);
check(Object.keys(await page.getSubmitted(tab("https://www.example.com/"))).length === 0 && !store.has("pending_7"), "expires after 15 min");

await page.setSubmitted(tab("https://www.example.com/signup"), [true, "u", "P", "https://www.example.com", [], true]);
await page.clearSubmittedCredentials(tab("https://www.example.com/"));
check(!store.has("pending_7"), "page_clear_submitted removes it");
check(Object.keys(await page.getSubmitted({ id: 7 })).length === 0, "missing tab.url does not throw");

const site = tab("https://www.example.com/x");
await page.setSubmitted(site, [true, "step1@x.fr", "", "https://www.example.com", []]);
check((await page.getSubmitted(site)).username === "step1@x.fr", "username-only step is stored when nothing is pending");
await page.setSubmitted(site, [true, "", "GEN", "https://www.example.com", [], true]);
await page.setSubmitted(site, [true, "", "", "https://www.example.com", []]);
p = await page.getSubmitted(site);
check(p.password === "GEN" && p.generated && p.username === "step1@x.fr", "a password-less submit does not wipe the pending password");

await page.updateSubmittedUsername(site, ["luca@x.fr", "GEN"]);
p = await page.getSubmitted(site);
check(p.username === "luca@x.fr" && p.password === "GEN" && p.generated, "unload re-read updates the username of the same pending password");
await page.updateSubmittedUsername(site, ["other@x.fr", "OTHER"]);
check((await page.getSubmitted(site)).username === "luca@x.fr", "unload re-read with another password is a no-op");
await page.clearSubmittedCredentials(site);
await page.updateSubmittedUsername(site, ["luca@x.fr", "GEN"]);
check(!store.has("pending_7"), "unload re-read never revives a cleared pending save");

// Ignored API keys: SHA-256 fingerprints in session storage, shared by every tab
const fp = "ab".repeat(32);
check(!(await page.isApiKeyIgnored(site, [fp])), "unknown fingerprint is not ignored");
await page.ignoreApiKey(site, [fp]);
await page.ignoreApiKey(tab("https://other.org/", 9), [fp]);
check(await page.isApiKeyIgnored({ id: 3 }, [fp]) && store.get("ignored_api_keys").length === 1, "ignored in every tab, stored once");
await page.ignoreApiKey(site, ["sk-proj-ThisIsAClearKeyNotAHash1234"]);
check(store.get("ignored_api_keys").length === 1, "only SHA-256 hex fingerprints are stored, never a clear key");

console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
