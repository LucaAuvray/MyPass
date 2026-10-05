// Runnable check for import deduplication and file decoding: `node src/lib/dedup.check.ts`
// (Node strips the types itself; the project has no front-end test runner).
import { computeDuplicateGroups, decodeImportBytes, vaultEntryToDupEntry } from "./dedup.ts";
import type { ParsedEntry } from "@/types/import";

let failed = 0;
const check = (ok: boolean, msg: string) => {
  console.log(`${ok ? "ok  " : "FAIL"} - ${msg}`);
  if (!ok) failed++;
};

const card = (tempId: string, number: string): ParsedEntry => ({
  tempId,
  group: "",
  title: "Visa",
  username: "",
  password: "",
  url: "",
  notes: "",
  tags: [],
  totp: "",
  customFields: { MyPass_Type: "card", CC_Number: number },
});

check(
  computeDuplicateGroups([card("a", "4242"), card("b", "5555")], []).length === 0,
  "two different cards named Visa are not duplicates",
);
const vaultCard = vaultEntryToDupEntry({
  uuid: "v1",
  title: "Visa",
  username: "",
  password: "",
  url: "",
  customFields: { MyPass_Type: "card", CC_Number: "4242" },
});
check(
  computeDuplicateGroups([card("a", "4242")], [vaultCard]).length === 1,
  "the same card re-imported duplicates the vault's",
);
check(
  vaultEntryToDupEntry({
    uuid: "v2",
    title: "x",
    username: "u",
    password: "",
    url: "",
    hasTotp: true,
  }).hasTotp,
  "the vault side of a duplicate shows its TOTP",
);
check(
  decodeImportBytes(new Uint8Array([0x70, 0x61, 0x73, 0x73, 0xe9])) === "passé",
  "Windows-1252 CSV (Excel FR) decoded",
);
check(decodeImportBytes(new TextEncoder().encode("passé")) === "passé", "UTF-8 file decoded");

if (failed) throw new Error(`${failed} check(s) failed`);
console.log("DEDUP CHECK OK");
