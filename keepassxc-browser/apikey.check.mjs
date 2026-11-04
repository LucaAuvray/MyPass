// Runnable check of the API key proposal throttle (content/apikey.js) with stubs: `node keepassxc-browser/apikey.check.mjs`
import { readFileSync } from "node:fs";
import vm from "node:vm";

const src = readFileSync(new URL("./keepassxc-browser/content/apikey.js", import.meta.url), "utf8");
let inFlight = 0, maxInFlight = 0, checks = 0, banners = 0;
const sendMessage = async (action) => {
    inFlight++; maxInFlight = Math.max(maxInFlight, inFlight);
    await new Promise((r) => setTimeout(r, 1));
    inFlight--;
    if (action === "check_api_key") { checks++; return false; }
    if (action === "get_status") return { keePassXCAvailable: true, databaseClosed: false };
    return false;
};
const kpxcBanner = { apiKey: undefined, createApiKey: async (item) => { banners++; kpxcBanner.apiKey = item; } };
const ctx = { sendMessage, kpxcBanner, crypto, TextEncoder, Promise, setTimeout, logError: console.error,
    window: { top: { location: { href: "https://console.example.ai/keys?x=1#y" } } } };
const k = vm.runInNewContext(`${src}\n;kpxcApiKey`, ctx);

let failed = 0;
const check = (ok, msg) => { console.log(`${ok ? "ok  " : "FAIL"} - ${msg}`); if (!ok) failed++; };

// A flood of distinct candidates while the first one is being offered
for (let i = 0; i < 1000; i++) k.propose(`key-${i}`);
await k.chain;
check(maxInFlight === 1, "offers run one at a time");
check(k.seen.size === 50, "at most 50 candidates per page");
check(banners === 1 && checks === 1, "the first key gets the banner, later ones are skipped without asking the app");
check(kpxcBanner.apiKey.url === "https://console.example.ai/keys", "saved URL drops the query and the hash");

kpxcBanner.apiKey = undefined;
k.propose("key-0");
await k.chain;
check(banners === 1, "a key already proposed on this page is not proposed again");

console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
