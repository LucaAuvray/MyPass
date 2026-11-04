// Runnable check of the API key proposal throttle (content/apikey.js) with stubs: `node keepassxc-browser/apikey.check.mjs`
import { readFileSync } from "node:fs";
import vm from "node:vm";

const src = readFileSync(new URL("./keepassxc-browser/content/apikey.js", import.meta.url), "utf8");
let inFlight = 0, maxInFlight = 0, checks = 0, banners = 0, failNext = false;
const sendMessage = async (action) => {
    inFlight++; maxInFlight = Math.max(maxInFlight, inFlight);
    await new Promise((r) => setTimeout(r, 1));
    inFlight--;
    if (action === "check_api_key" && failNext) { failNext = false; throw new Error("host gone"); }
    if (action === "check_api_key") { checks++; return false; }
    if (action === "get_status") return { keePassXCAvailable: true, databaseClosed: false };
    return false;
};
const kpxcBanner = { apiKey: undefined, createApiKey: async (item) => { banners++; kpxcBanner.apiKey = item; } };
const ctx = { sendMessage, kpxcBanner, crypto, TextEncoder, Promise, setTimeout, logError: () => {},
    window: { top: { location: { href: "https://console.example.ai/keys?x=1#y" } } } };
const k = vm.runInNewContext(`${src}\n;kpxcApiKey`, ctx);

let failed = 0;
const check = (ok, msg) => { console.log(`${ok ? "ok  " : "FAIL"} - ${msg}`); if (!ok) failed++; };

// A flood of distinct candidates while the first one is being offered
for (let i = 0; i < 1000; i++) k.propose(`key-${i}`);
await k.chain;
check(maxInFlight === 1, "offers run one at a time");
check(banners === 1 && checks === 1, "the first key gets the banner, later ones are not sent to the app");
check(k.seen.size === 1 && k.waiting.size === 3, "behind the banner at most 3 keys wait, the rest of the flood is dropped");
check(kpxcBanner.apiKey.url === "https://console.example.ai/keys", "saved URL drops the query and the hash");

// The banner closes (kpxcBanner.destroyApiKey): the waiting keys come next, one banner at a time
const close = async () => { kpxcBanner.apiKey = undefined; k.proposeWaiting(); await k.chain; };
await close();
check(banners === 2 && checks === 2 && kpxcBanner.apiKey.key === "key-1" && k.waiting.size === 2,
    "after the banner closes the next waiting key gets it, the others wait again");
await close();
await close();
check(banners === 4 && k.waiting.size === 0, "the flood ends after 3 more banners");
await close();
check(banners === 4 && k.seen.size === 4, "then nothing, and only offered keys count toward the cap");

kpxcBanner.apiKey = { key: "shown" };
k.propose("copied-key");
check(k.waiting.has("copied-key") && !k.seen.has("copied-key"), "a key copied while a banner is shown waits for it");
k.waiting.clear();
kpxcBanner.apiKey = undefined;

k.propose("key-0");
await k.chain;
check(banners === 4, "a key already proposed on this page is not proposed again");

failNext = true;
k.propose("flaky-key");
await k.chain;
check(banners === 4 && !k.seen.has("flaky-key"), "an offer that fails forgets the key");
k.propose("flaky-key");
await k.chain;
check(banners === 5 && kpxcBanner.apiKey.key === "flaky-key", "so a later scan or Copy offers it again");

console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
