// Runnable check for the folder tree helpers: `node src/lib/groups.check.ts`
// (Node strips the types itself; the project has no front-end test runner).
import { findGroup, flattenGroups, groupAndDescendants } from "./groups.ts";
import type { Group } from "@/types/group";

let failed = 0;
const check = (ok: boolean, msg: string) => {
  console.log(`${ok ? "ok  " : "FAIL"} - ${msg}`);
  if (!ok) failed++;
};

const g = (uuid: string, children: Group[] = []): Group => ({
  uuid,
  name: uuid,
  icon: null,
  children,
  entryCount: 0,
  isExpanded: true,
  created: null,
  modified: null,
});
const root = g("root", [g("A", [g("A1", [g("A1a")])]), g("B")]);
const eq = (set: Set<string> | null, uuids: string[]) =>
  set !== null && set.size === uuids.length && uuids.every((u) => set.has(u));

check(findGroup(root, "A1")?.name === "A1", "findGroup finds a nested folder");
check(findGroup(root, "zz") === null, "findGroup returns null when missing");
check(eq(groupAndDescendants(root, "A"), ["A", "A1", "A1a"]), "folder + all descendants");
check(eq(groupAndDescendants(root, "B"), ["B"]), "leaf folder");
check(groupAndDescendants(root, "zz") === null, "missing folder → null");
check(
  JSON.stringify(flattenGroups(root).map((f) => [f.uuid, f.depth])) ===
    JSON.stringify([
      ["A", 0],
      ["A1", 1],
      ["A1a", 2],
      ["B", 0],
    ]),
  "pre-order, root excluded",
);

if (failed) throw new Error(`${failed} check(s) failed`);
console.log("GROUPS CHECK OK");
