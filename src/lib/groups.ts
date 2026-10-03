import type { Group } from "@/types/group";

/** The folder `uuid` anywhere under `root` (root included), or null. */
export function findGroup(root: Group, uuid: string): Group | null {
  if (root.uuid === uuid) return root;
  for (const child of root.children) {
    const found = findGroup(child, uuid);
    if (found) return found;
  }
  return null;
}

/** The uuids of folder `uuid` and of all its subfolders, or null when it does not exist. */
export function groupAndDescendants(root: Group, uuid: string): Set<string> | null {
  const group = findGroup(root, uuid);
  if (!group) return null;
  const uuids = new Set<string>();
  const walk = (g: Group) => {
    uuids.add(g.uuid);
    g.children.forEach(walk);
  };
  walk(group);
  return uuids;
}

/** Every folder under `root` in tree order (root excluded), top-level folders at depth 0. */
export function flattenGroups(root: Group): { uuid: string; name: string; depth: number }[] {
  const out: { uuid: string; name: string; depth: number }[] = [];
  const walk = (g: Group, depth: number) => {
    out.push({ uuid: g.uuid, name: g.name, depth });
    g.children.forEach((c) => walk(c, depth + 1));
  };
  root.children.forEach((c) => walk(c, 0));
  return out;
}
