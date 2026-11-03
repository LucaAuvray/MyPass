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

console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
