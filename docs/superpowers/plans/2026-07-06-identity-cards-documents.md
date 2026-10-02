# Identités, cartes bancaires et documents — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stocker identités, cartes bancaires et documents dans le coffre KDBX de MyPass, avec UI dédiée (copie un clic) et remplissage de formulaires web en un clic via l'extension navigateur.

**Architecture:** Les éléments sont des entrées KDBX standard discriminées par le champ personnalisé `MyPass_Type` (`identity`|`card`|`document`). Le frontend React filtre/affiche par type ; une nouvelle action Native Messaging `get-identities` expose identités et cartes (jamais les documents) au fork keepassxc-browser, où un nouveau content script détecte les champs de formulaire (attribut `autocomplete` + heuristiques FR/EN) et remplit au clic.

**Tech Stack:** React 19 + Zustand + TanStack Query + react-hook-form + shadcn/ui + react-i18next ; Rust (Tauri v2, serde) ; extension WebExtension MV3 (fork keepassxc-browser).

**Spec:** `docs/superpowers/specs/2026-07-06-identity-cards-documents-design.md`

## Global Constraints

- **Git** : dépôt initialisé le 2026-07-06 (baseline `d82a1aa` sur `main`), travail sur la branche `feature/identity-cards-documents` — un commit par tâche. Le `.git` du clone `keepassxc-browser/` a été renommé `.git-upstream` pour que le dépôt racine suive le fork de l'extension ; ne pas y toucher. `keepassxc/` reste hors git.
- **Pas de test runner frontend** : la vérification frontend = `npm run build` (tsc + vite, doit passer sans erreur) + vérification manuelle dans l'app.
- **`npm run dev` utilise un backend mock** (`src/lib/tauri.ts`) : tout ce qui touche Rust/KDBX se vérifie avec `npm run tauri dev`.
- **Rust** : `cd src-tauri && cargo test` doit passer ; `cargo clippy` sans nouveau warning.
- **i18n obligatoire** : toute chaîne UI passe par `react-i18next` (`src/i18n/en.json` + `fr.json`).
- **Lint** : `npm run lint` passe avec `--max-warnings 0`.
- **Extension active** = `keepassxc-browser/keepassxc-browser/` (PAS `extension/`).
- Champs protégés dans le KDBX : `CC_Number`, `CC_CVC`, `DOC_Number`.
- Les documents ne sont **jamais** renvoyés par `get-identities`.

---

### Task 1 : Helpers TypeScript des types d'éléments

**Files:**
- Create: `src/lib/items.ts`

**Interfaces:**
- Consumes: rien (module feuille, aucune dépendance interne).
- Produces: `ItemKind`, `MYPASS_TYPE_KEY`, `IDENTITY_FIELDS`, `CARD_FIELDS`, `DOCUMENT_FIELDS`, `SECRET_FIELDS`, `DOC_KINDS`, `itemKind(entry)`, `cardBrand(number)`, `maskCardNumber(number)` — utilisés par les tâches 4, 5, 6.

- [ ] **Step 1 : Créer `src/lib/items.ts`**

```ts
/** Types d'éléments MyPass (identités / cartes / documents), stockés en
 *  entrées KDBX discriminées par le champ personnalisé MyPass_Type. */

export type ItemKind = "login" | "identity" | "card" | "document";

export const MYPASS_TYPE_KEY = "MyPass_Type";

export const IDENTITY_FIELDS = [
  "ID_FirstName",
  "ID_LastName",
  "ID_BirthDate",
  "ID_Email",
  "ID_Phone",
  "ID_Address",
  "ID_City",
  "ID_PostalCode",
  "ID_Country",
  "ID_Company",
] as const;

export const CARD_FIELDS = [
  "CC_Holder",
  "CC_Number",
  "CC_ExpMonth",
  "CC_ExpYear",
  "CC_CVC",
] as const;

export const DOCUMENT_FIELDS = [
  "DOC_Kind",
  "DOC_Number",
  "DOC_IssueDate",
  "DOC_ExpiryDate",
  "DOC_Country",
] as const;

/** Masqués par défaut dans l'UI ; protégés côté KDBX (voir entries.rs). */
export const SECRET_FIELDS: string[] = ["CC_Number", "CC_CVC", "DOC_Number"];

export const DOC_KINDS = [
  "passport",
  "id_card",
  "driver_license",
  "social_security",
  "other",
] as const;

export function itemKind(entry: { customFields?: Record<string, string> }): ItemKind {
  const t = entry.customFields?.[MYPASS_TYPE_KEY];
  return t === "identity" || t === "card" || t === "document" ? t : "login";
}

export function cardBrand(number: string): "visa" | "mastercard" | "amex" | "other" {
  const n = number.replace(/\s/g, "");
  if (n.startsWith("4")) return "visa";
  if (/^(5[1-5]|2[2-7])/.test(n)) return "mastercard";
  if (/^3[47]/.test(n)) return "amex";
  return "other";
}

export function maskCardNumber(number: string): string {
  const n = number.replace(/\s/g, "");
  return n.length >= 4 ? `•••• ${n.slice(-4)}` : "••••";
}
```

- [ ] **Step 2 : Vérifier**

Run: `npm run build` — attendu : succès sans erreur TS.
Run: `npm run lint` — attendu : 0 warning.

---

### Task 2 : Rust — faire circuler les customFields (create/read/update)

Aujourd'hui `EntryInfo` (lecture), `NewEntry` (création) et `UpdateEntry` (mise à jour) ignorent les champs personnalisés : rien ne circule entre le frontend et le XML KDBX. Cette tâche les fait transiter, avec protection des champs sensibles.

**Files:**
- Modify: `src-tauri/src/commands/entries.rs`

**Interfaces:**
- Consumes: `xml::Entry`, `EntryString`, `Value`, `set_string_field`, `set_string_field_protected` (déjà dans le fichier).
- Produces: `EntryInfo.custom_fields` (sérialisé `customFields`), `NewEntry.custom_fields` / `UpdateEntry.custom_fields` (désérialisés depuis `customFields` camelCase), `apply_custom_fields(&mut Entry, &HashMap<String,String>)` (pub(crate), réutilisé par les tests de la tâche 7).

**Attention :** on ajoute `#[serde(rename_all = "camelCase")]` sur `NewEntry`/`UpdateEntry`. Effet de bord voulu : `groupUuid` envoyé par le frontend, silencieusement ignoré jusqu'ici (serde attendait `group_uuid`), sera enfin désérialisé.

- [ ] **Step 1 : Écrire les tests (qui échouent) en bas de `entries.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn card_fields() -> HashMap<String, String> {
        let mut f = HashMap::new();
        f.insert("MyPass_Type".to_string(), "card".to_string());
        f.insert("CC_Number".to_string(), "4242424242424242".to_string());
        f.insert("CC_CVC".to_string(), "123".to_string());
        f.insert("CC_Holder".to_string(), "Luca Auvray".to_string());
        f
    }

    #[test]
    fn apply_custom_fields_protects_secret_keys_only() {
        let mut entry = Entry::new("Visa perso", "", "", "");
        apply_custom_fields(&mut entry, &card_fields());

        let get = |key: &str| entry.strings.iter().find(|s| s.key == key).unwrap();
        assert_eq!(get("MyPass_Type").value.content, "card");
        assert_eq!(get("MyPass_Type").value.protected, None);
        assert_eq!(get("CC_Holder").value.protected, None);
        assert_eq!(get("CC_Number").value.protected.as_deref(), Some("True"));
        assert_eq!(get("CC_CVC").value.protected.as_deref(), Some("True"));
    }

    #[test]
    fn apply_custom_fields_updates_and_removes() {
        let mut entry = Entry::new("Visa perso", "", "", "");
        apply_custom_fields(&mut entry, &card_fields());

        let mut update = HashMap::new();
        update.insert("CC_Holder".to_string(), "L. Auvray".to_string());
        update.insert("CC_CVC".to_string(), String::new()); // vide => suppression
        apply_custom_fields(&mut entry, &update);

        let holder = entry.strings.iter().find(|s| s.key == "CC_Holder").unwrap();
        assert_eq!(holder.value.content, "L. Auvray");
        assert!(entry.strings.iter().all(|s| s.key != "CC_CVC"));
    }

    #[test]
    fn entry_to_info_exposes_custom_fields_without_standard_keys() {
        let mut entry = Entry::new("Visa perso", "user", "pass", "https://x.io");
        apply_custom_fields(&mut entry, &card_fields());

        let info = entry_to_info(&entry, "g1");
        assert_eq!(info.custom_fields.get("MyPass_Type").unwrap(), "card");
        assert_eq!(info.custom_fields.get("CC_Number").unwrap(), "4242424242424242");
        // Les clés standard ne doivent pas fuiter dans customFields
        for std_key in ["Title", "UserName", "Password", "URL", "Notes"] {
            assert!(!info.custom_fields.contains_key(std_key));
        }
    }
}
```

- [ ] **Step 2 : Vérifier l'échec**

Run: `cd src-tauri && cargo test commands::entries`
Attendu : erreur de compilation — `apply_custom_fields` et `custom_fields` n'existent pas.

- [ ] **Step 3 : Implémenter**

En haut de `entries.rs`, ajouter l'import :

```rust
use std::collections::HashMap;
```

Ajouter la constante et le helper (près de `set_string_field`, en bas du fichier) :

```rust
/// Champs personnalisés stockés avec le flag Protected (comme les mots de passe).
const PROTECTED_CUSTOM_KEYS: &[&str] = &["CC_Number", "CC_CVC", "DOC_Number"];

/// Upsert des champs personnalisés ; une valeur vide supprime la clé.
pub(crate) fn apply_custom_fields(entry: &mut Entry, fields: &HashMap<String, String>) {
    for (key, value) in fields {
        if value.is_empty() {
            entry.strings.retain(|s| &s.key != key);
        } else if PROTECTED_CUSTOM_KEYS.contains(&key.as_str()) {
            set_string_field_protected(entry, key, value);
        } else {
            set_string_field(entry, key, value);
        }
    }
}
```

Dans `EntryInfo`, ajouter le champ :

```rust
    #[serde(rename = "customFields")]
    pub custom_fields: HashMap<String, String>,
```

Dans `entry_to_info`, ajouter au constructeur :

```rust
        custom_fields: entry
            .strings
            .iter()
            .filter(|s| !matches!(s.key.as_str(), "Title" | "UserName" | "Password" | "URL" | "Notes"))
            .map(|s| (s.key.clone(), s.value.content.clone()))
            .collect(),
```

Sur `NewEntry` et `UpdateEntry`, ajouter l'attribut de struct et le champ :

```rust
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEntry {
    // ... champs existants inchangés ...
    #[serde(default)]
    pub custom_fields: Option<HashMap<String, String>>,
}
```

(idem pour `UpdateEntry`).

Dans `create_entry`, après `let new_entry = xml::Entry::new(...)` (passer `new_entry` en `mut`) :

```rust
    let mut new_entry = xml::Entry::new(
        &entry.title,
        &entry.username,
        &entry.password,
        &entry.url.unwrap_or_default(),
    );

    if let Some(fields) = &entry.custom_fields {
        apply_custom_fields(&mut new_entry, fields);
    }
```

Dans `update_entry`, après le bloc `if let Some(tags)` :

```rust
    if let Some(fields) = &update.custom_fields {
        apply_custom_fields(entry, fields);
    }
```

- [ ] **Step 4 : Vérifier**

Run: `cd src-tauri && cargo test commands::entries`
Attendu : 3 tests PASS.
Run: `cd src-tauri && cargo clippy` — attendu : aucun nouveau warning.

---

### Task 3 : Plomberie frontend — customFields dans le mock et les mutations

**Files:**
- Modify: `src/lib/tauri.ts` (mock `create_entry`, ~ligne 119)
- Modify: `src/hooks/useEntries.ts` (type des params de `createMutation`, ~ligne 27)

**Interfaces:**
- Consumes: types existants.
- Produces: `createEntry({ ..., customFields? })` accepté par le hook (utilisé par la tâche 5).

- [ ] **Step 1 : Mock — passer les customFields à la création**

Dans `src/lib/tauri.ts`, cas `create_entry`, remplacer `customFields: {},` par :

```ts
          customFields: (entry?.customFields as Record<string, string>) ?? {},
```

(Le cas `update_entry` fait déjà un spread de `update`, rien à changer.)

- [ ] **Step 2 : Hook — accepter customFields**

Dans `src/hooks/useEntries.ts`, dans le type des params de `createMutation.mutationFn`, ajouter après `tags?: string[];` :

```ts
      customFields?: Record<string, string>;
```

- [ ] **Step 3 : Vérifier**

Run: `npm run build` — attendu : succès.

---

### Task 4 : Filtre par type — store, barre latérale, liste

**Files:**
- Modify: `src/stores/entriesStore.ts`
- Modify: `src/components/layout/AppSidebar.tsx`
- Modify: `src/components/entries/EntryList.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/fr.json`

**Interfaces:**
- Consumes: `itemKind`, `ItemKind` de `@/lib/items` (Task 1).
- Produces: `useEntriesStore` expose `kindFilter: ItemKind | null` et `setKindFilter(kind)` (utilisés par les tâches 5/6 pour présélectionner le type).

- [ ] **Step 1 : Store**

Dans `src/stores/entriesStore.ts` :

```ts
import type { ItemKind } from "@/lib/items";
```

Dans `EntriesState`, ajouter :

```ts
  kindFilter: ItemKind | null;
  setKindFilter: (kind: ItemKind | null) => void;
```

Dans le `create(...)`, ajouter :

```ts
  kindFilter: null,
  setKindFilter: (kind) => set({ kindFilter: kind, selectedEntryId: null }),
```

- [ ] **Step 2 : Barre latérale**

Dans `AppSidebar.tsx` :
- Ajouter aux imports lucide : `Contact, CreditCard, FileText`.
- Récupérer le filtre : `const kindFilter = useEntriesStore((s) => s.kindFilter);` et `const setKindFilter = useEntriesStore((s) => s.setKindFilter);`
- Dans le `<nav>` du bas, remplacer la ligne `allItems` et insérer les trois sections après elle :

```tsx
              <SidebarItem icon={FolderOpen} label={t("nav.allItems")} active={location.pathname === "/" && !kindFilter} onClick={() => { navigate("/"); setKindFilter(null); selectEntry(null); }} />
              <SidebarItem icon={Contact} label={t("nav.identities")} active={location.pathname === "/" && kindFilter === "identity"} onClick={() => { navigate("/"); setKindFilter("identity"); }} />
              <SidebarItem icon={CreditCard} label={t("nav.cards")} active={location.pathname === "/" && kindFilter === "card"} onClick={() => { navigate("/"); setKindFilter("card"); }} />
              <SidebarItem icon={FileText} label={t("nav.documents")} active={location.pathname === "/" && kindFilter === "document"} onClick={() => { navigate("/"); setKindFilter("document"); }} />
```

- [ ] **Step 3 : Filtrage de la liste**

Dans `EntryList.tsx` :

```ts
import { itemKind } from "@/lib/items";
```

```ts
  const kindFilter = useEntriesStore((s) => s.kindFilter);

  const byKind = kindFilter
    ? entries.filter((e) => itemKind(e) === kindFilter)
    : entries;

  const filtered = searchQuery
    ? byKind.filter(
        (e) =>
          e.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
          e.username.toLowerCase().includes(searchQuery.toLowerCase()) ||
          e.url.toLowerCase().includes(searchQuery.toLowerCase()),
      )
    : byKind;
```

Remplacer aussi la condition d'état vide `entries.length === 0` par `byKind.length === 0` (sinon une section vide affiche « aucun résultat » au lieu de l'état vide).

- [ ] **Step 4 : i18n**

Dans `en.json`, section `nav`, ajouter :

```json
    "identities": "Identities",
    "cards": "Cards",
    "documents": "Documents"
```

Dans `fr.json`, section `nav` :

```json
    "identities": "Identités",
    "cards": "Cartes",
    "documents": "Documents"
```

- [ ] **Step 5 : Vérifier**

Run: `npm run build && npm run lint` — attendu : succès.
Manuel : `npm run dev`, ouvrir le coffre mock → les trois sections apparaissent, chacune affiche un état vide (aucun élément typé encore).

---

### Task 5 : Formulaire de création/édition des éléments (ItemForm) + menu « Nouvel élément »

**Files:**
- Create: `src/components/entries/ItemForm.tsx`
- Modify: `src/components/layout/AppSidebar.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/fr.json`

**Interfaces:**
- Consumes: `IDENTITY_FIELDS`, `CARD_FIELDS`, `DOCUMENT_FIELDS`, `DOC_KINDS`, `MYPASS_TYPE_KEY`, `cardBrand` (Task 1) ; `useEntries().createEntry/updateEntry` (Task 3) ; primitives shadcn existantes (`dialog`, `input`, `label`, `select`, `textarea`, `button`).
- Produces: `<ItemForm kind open onOpenChange editEntry? />` avec `editEntry?: { uuid: string; title: string; notes: string; customFields: Record<string, string> }` — utilisé par la tâche 6 pour l'édition.

- [ ] **Step 1 : Créer `src/components/entries/ItemForm.tsx`**

```tsx
import { useEffect } from "react";
import { useForm, Controller } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useEntries } from "@/hooks/useEntries";
import {
  type ItemKind,
  IDENTITY_FIELDS,
  CARD_FIELDS,
  DOCUMENT_FIELDS,
  DOC_KINDS,
  MYPASS_TYPE_KEY,
} from "@/lib/items";

type ItemFormValues = { title: string; notes: string } & Record<string, string>;

interface ItemFormProps {
  kind: Exclude<ItemKind, "login">;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  editEntry?: { uuid: string; title: string; notes: string; customFields: Record<string, string> };
}

const KIND_FIELDS: Record<Exclude<ItemKind, "login">, readonly string[]> = {
  identity: IDENTITY_FIELDS,
  card: CARD_FIELDS,
  document: DOCUMENT_FIELDS,
};

export function ItemForm({ kind, open, onOpenChange, editEntry }: ItemFormProps) {
  const { t } = useTranslation();
  const { createEntry, updateEntry } = useEntries();
  const isEditing = !!editEntry;
  const fields = KIND_FIELDS[kind];

  const form = useForm<ItemFormValues>({ defaultValues: defaultsFrom(editEntry, fields) });

  // Réinitialise quand on ouvre pour un autre élément / une création
  useEffect(() => {
    if (open) form.reset(defaultsFrom(editEntry, fields));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, editEntry?.uuid]);

  const onSubmit = async (data: ItemFormValues) => {
    const customFields: Record<string, string> = { [MYPASS_TYPE_KEY]: kind };
    for (const key of fields) {
      // valeur vide => suppression côté backend en édition ; ignorée à la création
      customFields[key] = (data[key] ?? "").trim();
    }
    if (isEditing && editEntry) {
      await updateEntry({
        uuid: editEntry.uuid,
        update: { title: data.title, notes: data.notes, customFields },
      });
    } else {
      await createEntry({
        title: data.title,
        username: "",
        password: "",
        notes: data.notes,
        customFields: Object.fromEntries(Object.entries(customFields).filter(([, v]) => v !== "")),
      });
    }
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {isEditing ? t("items.editTitle", { kind: t(`items.${kind}`) }) : t(`items.new.${kind}`)}
          </DialogTitle>
          <DialogDescription>{t(`items.desc.${kind}`)}</DialogDescription>
        </DialogHeader>

        <form onSubmit={form.handleSubmit(onSubmit)} className="flex flex-col gap-4">
          <div className="space-y-1.5">
            <Label htmlFor="item-title">{t("entries.title")}</Label>
            <Input
              id="item-title"
              autoFocus
              {...form.register("title", { required: t("entries.titleRequired") })}
              placeholder={t(`items.titlePlaceholder.${kind}`)}
            />
            {form.formState.errors.title && (
              <p className="text-xs text-destructive">{form.formState.errors.title.message}</p>
            )}
          </div>

          <div className="grid grid-cols-2 gap-3">
            {fields.map((key) => (
              <ItemField key={key} fieldKey={key} form={form} t={t} />
            ))}
          </div>

          <div className="space-y-1.5">
            <Label htmlFor="item-notes">{t("entries.notes")}</Label>
            <Textarea id="item-notes" rows={2} {...form.register("notes")} />
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              {t("entries.cancel")}
            </Button>
            <Button type="submit">{isEditing ? t("entries.save") : t("entries.create")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function defaultsFrom(
  editEntry: ItemFormProps["editEntry"],
  fields: readonly string[],
): ItemFormValues {
  const values: ItemFormValues = {
    title: editEntry?.title ?? "",
    notes: editEntry?.notes ?? "",
  };
  for (const key of fields) values[key] = editEntry?.customFields[key] ?? "";
  return values;
}

/** Un champ d'élément : select pour DOC_Kind, input typé sinon. */
function ItemField({
  fieldKey,
  form,
  t,
}: {
  fieldKey: string;
  form: ReturnType<typeof useForm<ItemFormValues>>;
  t: (key: string) => string;
}) {
  const label = t(`items.fields.${fieldKey}`);
  const wide = fieldKey === "ID_Address" || fieldKey === "CC_Number";
  const isDate = fieldKey.endsWith("Date");
  const isSecret = fieldKey === "CC_Number" || fieldKey === "CC_CVC" || fieldKey === "DOC_Number";

  if (fieldKey === "DOC_Kind") {
    return (
      <div className="space-y-1.5">
        <Label>{label}</Label>
        <Controller
          control={form.control}
          name={fieldKey}
          render={({ field }) => (
            <Select value={field.value || undefined} onValueChange={field.onChange}>
              <SelectTrigger>
                <SelectValue placeholder={label} />
              </SelectTrigger>
              <SelectContent>
                {DOC_KINDS.map((k) => (
                  <SelectItem key={k} value={k}>
                    {t(`items.docKinds.${k}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        />
      </div>
    );
  }

  return (
    <div className={wide ? "col-span-2 space-y-1.5" : "space-y-1.5"}>
      <Label htmlFor={`item-${fieldKey}`}>{label}</Label>
      <Input
        id={`item-${fieldKey}`}
        type={isDate ? "date" : "text"}
        inputMode={fieldKey === "CC_Number" || fieldKey === "CC_CVC" ? "numeric" : undefined}
        className={isSecret ? "font-mono" : undefined}
        maxLength={fieldKey === "CC_CVC" ? 4 : fieldKey === "CC_ExpMonth" || fieldKey === "CC_ExpYear" ? 2 : undefined}
        placeholder={fieldKey === "CC_ExpMonth" ? "MM" : fieldKey === "CC_ExpYear" ? "AA" : undefined}
        {...form.register(fieldKey)}
      />
    </div>
  );
}
```

Note : `CC_Type` n'est pas saisi — il est dérivable à l'affichage via `cardBrand(CC_Number)` ; on ne le stocke pas (YAGNI, une source de vérité).
**Répercuter dans la spec/extension : ne jamais lire `CC_Type` stocké.**

- [ ] **Step 2 : Menu « Nouvel élément » dans la barre latérale**

Dans `AppSidebar.tsx` :
- Imports : `DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger` depuis `@/components/ui/dropdown-menu` ; `ItemForm` depuis `@/components/entries/ItemForm` ; `KeyRound` ajouté aux imports lucide ; `type ItemKind` depuis `@/lib/items`.
- État : `const [newItemKind, setNewItemKind] = useState<Exclude<ItemKind, "login"> | null>(null);`
- À côté de `<EntryForm ... />`, ajouter :

```tsx
      {newItemKind && (
        <ItemForm
          kind={newItemKind}
          open={!!newItemKind}
          onOpenChange={(o) => !o && setNewItemKind(null)}
        />
      )}
```

- Remplacer le bouton « New Entry » (le `<button onClick={() => setShowNewEntry(true)} ...>`) par :

```tsx
              <DropdownMenu>
                <DropdownMenuTrigger className="flex w-full items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90">
                  <Plus className="size-4" />
                  {t("nav.newItem")}
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="w-52">
                  <DropdownMenuItem onClick={() => setShowNewEntry(true)}>
                    <KeyRound className="size-4" /> {t("nav.newPassword")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setNewItemKind("identity")}>
                    <Contact className="size-4" /> {t("items.new.identity")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setNewItemKind("card")}>
                    <CreditCard className="size-4" /> {t("items.new.card")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setNewItemKind("document")}>
                    <FileText className="size-4" /> {t("items.new.document")}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
```

- [ ] **Step 3 : i18n — section `items` complète**

Dans `en.json`, ajouter la section racine `items` et la clé `nav.newItem: "New item"` :

```json
  "items": {
    "identity": "Identity",
    "card": "Bank card",
    "document": "Document",
    "editTitle": "Edit {{kind}}",
    "new": { "identity": "New identity", "card": "New card", "document": "New document" },
    "desc": {
      "identity": "Personal details for filling sign-up forms.",
      "card": "Card details for filling payment forms.",
      "document": "Official document details, stored encrypted."
    },
    "titlePlaceholder": { "identity": "Personal identity", "card": "Personal Visa", "document": "Passport" },
    "fields": {
      "ID_FirstName": "First name", "ID_LastName": "Last name", "ID_BirthDate": "Birth date",
      "ID_Email": "Email", "ID_Phone": "Phone", "ID_Address": "Address", "ID_City": "City",
      "ID_PostalCode": "Postal code", "ID_Country": "Country", "ID_Company": "Company",
      "CC_Holder": "Cardholder", "CC_Number": "Card number", "CC_ExpMonth": "Exp. month",
      "CC_ExpYear": "Exp. year", "CC_CVC": "CVC",
      "DOC_Kind": "Document type", "DOC_Number": "Number", "DOC_IssueDate": "Issue date",
      "DOC_ExpiryDate": "Expiry date", "DOC_Country": "Country"
    },
    "docKinds": {
      "passport": "Passport", "id_card": "ID card", "driver_license": "Driver's license",
      "social_security": "Social security", "other": "Other"
    }
  }
```

Dans `fr.json` (mêmes clés) :

```json
  "items": {
    "identity": "Identité",
    "card": "Carte bancaire",
    "document": "Document",
    "editTitle": "Modifier {{kind}}",
    "new": { "identity": "Nouvelle identité", "card": "Nouvelle carte", "document": "Nouveau document" },
    "desc": {
      "identity": "Coordonnées personnelles pour remplir les formulaires d'inscription.",
      "card": "Informations de carte pour remplir les formulaires de paiement.",
      "document": "Papiers officiels, stockés chiffrés."
    },
    "titlePlaceholder": { "identity": "Identité perso", "card": "Visa perso", "document": "Passeport" },
    "fields": {
      "ID_FirstName": "Prénom", "ID_LastName": "Nom", "ID_BirthDate": "Date de naissance",
      "ID_Email": "Email", "ID_Phone": "Téléphone", "ID_Address": "Adresse", "ID_City": "Ville",
      "ID_PostalCode": "Code postal", "ID_Country": "Pays", "ID_Company": "Société",
      "CC_Holder": "Titulaire", "CC_Number": "Numéro de carte", "CC_ExpMonth": "Mois d'exp.",
      "CC_ExpYear": "Année d'exp.", "CC_CVC": "CVC",
      "DOC_Kind": "Type de document", "DOC_Number": "Numéro", "DOC_IssueDate": "Date d'émission",
      "DOC_ExpiryDate": "Date d'expiration", "DOC_Country": "Pays"
    },
    "docKinds": {
      "passport": "Passeport", "id_card": "Carte d'identité", "driver_license": "Permis de conduire",
      "social_security": "Sécurité sociale", "other": "Autre"
    }
  }
```

Et dans les deux fichiers, section `nav` : `"newItem": "New item"` / `"newItem": "Nouvel élément"`.

- [ ] **Step 4 : Vérifier**

Run: `npm run build && npm run lint` — attendu : succès.
Manuel (`npm run dev`) : créer une identité, une carte, un document via le menu ; chacun apparaît dans sa section latérale ; rouvrir en édition n'écrase pas les champs.

---

### Task 6 : Affichage — EntryDetail et EntryCard par type + édition

**Files:**
- Modify: `src/components/entries/EntryDetail.tsx`
- Modify: `src/components/entries/EntryCard.tsx`

**Interfaces:**
- Consumes: `itemKind`, `maskCardNumber`, `cardBrand`, `SECRET_FIELDS`, `KIND_FIELDS` équivalents (Task 1), `ItemForm` (Task 5), `EntryForm`, `CopyButton`, `useReveal`.
- Produces: rien de nouveau pour les autres tâches.

- [ ] **Step 1 : EntryCard — sous-titre et icône par type**

Dans `EntryCard.tsx` :

```ts
import { itemKind, maskCardNumber } from "@/lib/items";
import { Contact, CreditCard, FileText } from "lucide-react";
```

Dans le composant, avant le `return` :

```tsx
  const kind = itemKind(entry);
  const cf = entry.customFields ?? {};
  const subtitle =
    kind === "card"
      ? `${maskCardNumber(cf.CC_Number ?? "")}${cf.CC_ExpMonth ? ` · ${cf.CC_ExpMonth}/${cf.CC_ExpYear ?? ""}` : ""}`
      : kind === "identity"
        ? cf.ID_Email || [cf.ID_FirstName, cf.ID_LastName].filter(Boolean).join(" ")
        : kind === "document"
          ? t(`items.docKinds.${cf.DOC_Kind || "other"}`)
          : entry.username || t("entries.noUsername");
```

Remplacer `<EntryIcon url={entry.url} size="md" />` par :

```tsx
        {kind === "login" ? (
          <EntryIcon url={entry.url} size="md" />
        ) : (
          <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-secondary text-muted-foreground">
            {kind === "identity" ? <Contact className="size-5" /> : kind === "card" ? <CreditCard className="size-5" /> : <FileText className="size-5" />}
          </div>
        )}
```

Et remplacer le `<p>` du sous-titre (`{entry.username || t("entries.noUsername")}`) par `{subtitle}`.

- [ ] **Step 2 : EntryDetail — champs par type + boutons copier + édition**

Dans `EntryDetail.tsx` :

Imports supplémentaires :

```ts
import { useState } from "react";
import { itemKind, IDENTITY_FIELDS, CARD_FIELDS, DOCUMENT_FIELDS, SECRET_FIELDS } from "@/lib/items";
import { Contact, CreditCard, FileText } from "lucide-react";
import { EntryForm } from "./EntryForm";
import { ItemForm } from "./ItemForm";
```

Dans le composant, après `const entry = entries.find(...)` (et après le guard `if (!entry)`) :

```tsx
  const kind = itemKind(entry);
```

État d'édition (en haut du composant, avec les autres hooks — les hooks doivent rester avant le guard `if (!entry)`) :

```tsx
  const [editOpen, setEditOpen] = useState(false);
```

Brancher le bouton crayon existant :

```tsx
          <Button variant="ghost" size="icon" className="size-8" onClick={() => setEditOpen(true)}>
            <Pencil className="size-4" />
          </Button>
```

À la fin du JSX racine (avant la fermeture du `div` racine), ajouter :

```tsx
      {kind === "login" ? (
        <EntryForm open={editOpen} onOpenChange={setEditOpen} editEntry={entry} />
      ) : (
        <ItemForm
          kind={kind}
          open={editOpen}
          onOpenChange={setEditOpen}
          editEntry={{ uuid: entry.uuid, title: entry.title, notes: entry.notes, customFields: entry.customFields ?? {} }}
        />
      )}
```

Remplacer le bloc `<CardContent>` : si `kind !== "login"`, rendre les champs de l'élément à la place des champs login. Garder `FieldRow` tel quel et ajouter :

```tsx
const KIND_FIELDS = { identity: IDENTITY_FIELDS, card: CARD_FIELDS, document: DOCUMENT_FIELDS } as const;

function ItemFields({ kind, customFields }: { kind: "identity" | "card" | "document"; customFields: Record<string, string> }) {
  const { t } = useTranslation();
  const { revealed, toggle } = useReveal();

  return (
    <>
      {KIND_FIELDS[kind].filter((key) => customFields[key]).map((key) => {
        const secret = SECRET_FIELDS.includes(key);
        const raw = key === "DOC_Kind" ? t(`items.docKinds.${customFields[key]}`) : customFields[key];
        return (
          <FieldRow
            key={key}
            icon={kind === "identity" ? <Contact className="size-4" /> : kind === "card" ? <CreditCard className="size-4" /> : <FileText className="size-4" />}
            label={t(`items.fields.${key}`)}
            value={secret && !revealed ? "••••••••" : raw}
            mono={secret}
            actions={
              <div className="flex items-center gap-0.5">
                <CopyButton text={customFields[key]} />
                {secret && (
                  <button onClick={toggle} className="rounded p-1 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground" aria-label={revealed ? t("unlock.hide") : t("unlock.show")}>
                    {revealed ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
                  </button>
                )}
              </div>
            }
          />
        );
      })}
    </>
  );
}
```

Dans le `<CardContent>`, envelopper les champs login existants (Username / Password / URL) dans `{kind === "login" && (<> ... </>)}` et ajouter :

```tsx
        {kind !== "login" && <ItemFields kind={kind} customFields={entry.customFields ?? {}} />}
```

(Le bloc Notes et les timestamps restent communs à tous les types. Le header : pour `kind !== "login"`, remplacer `<EntryIcon url={entry.url} size="lg" />` par la même pastille d'icône que dans EntryCard en taille `size-12`, et le sous-titre `{entry.url}` par `{t(`items.${kind}`)}`.)

- [ ] **Step 3 : Vérifier**

Run: `npm run build && npm run lint` — attendu : succès.
Manuel (`npm run dev`) : sélectionner une carte → numéro masqué, œil le révèle, chaque champ se copie ; crayon ouvre le formulaire pré-rempli ; les logins s'affichent comme avant.
Manuel (`npm run tauri dev`) : créer une carte, sauvegarder le coffre, verrouiller/déverrouiller → la carte est toujours là avec ses champs (round-trip KDBX réel, Task 2).

---

### Task 7 : Rust — action Native Messaging `get-identities`

**Files:**
- Modify: `src-tauri/src/commands/browser.rs`
- Modify: `src-tauri/src/native_messaging.rs`

**Interfaces:**
- Consumes: `collect_all_entries` (déjà dans browser.rs), `Entry::get_field` (xml.rs, pub), `apply_custom_fields` (Task 2, pour les tests), `decrypt_message` / `error_response` / `NativeResponse` (native_messaging.rs).
- Produces: `browser::handle_get_identities(&KeePassFile) -> Vec<serde_json::Value>` ; action protocole `"get-identities"` répondant `{ success: "true", identities: [{ uuid, title, type, fields }] }` (consommée par la tâche 8).

- [ ] **Step 1 : Test (qui échoue) dans `browser.rs`**

En bas du fichier :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::entries::apply_custom_fields;
    use crate::kdbx::xml::{Entry, KeePassFile};
    use std::collections::HashMap;

    fn entry_with_type(title: &str, item_type: &str, extra: &[(&str, &str)]) -> Entry {
        let mut e = Entry::new(title, "", "", "");
        let mut fields = HashMap::new();
        fields.insert("MyPass_Type".to_string(), item_type.to_string());
        for (k, v) in extra {
            fields.insert(k.to_string(), v.to_string());
        }
        apply_custom_fields(&mut e, &fields);
        e
    }

    #[test]
    fn get_identities_returns_only_identities_and_cards() {
        let mut kf = KeePassFile::new("test");
        kf.root.group.entries.push(Entry::new("Login GitHub", "dev", "pw", "https://github.com"));
        kf.root.group.entries.push(entry_with_type("Identité perso", "identity", &[("ID_FirstName", "Luca"), ("ID_Email", "luca@example.com")]));
        kf.root.group.entries.push(entry_with_type("Visa perso", "card", &[("CC_Number", "4242424242424242"), ("CC_Holder", "Luca Auvray")]));
        kf.root.group.entries.push(entry_with_type("Passeport", "document", &[("DOC_Number", "12AB34567")]));

        let result = handle_get_identities(&kf);

        assert_eq!(result.len(), 2);
        let types: Vec<&str> = result.iter().map(|v| v["type"].as_str().unwrap()).collect();
        assert!(types.contains(&"identity") && types.contains(&"card"));

        let card = result.iter().find(|v| v["type"] == "card").unwrap();
        assert_eq!(card["title"], "Visa perso");
        assert_eq!(card["fields"]["CC_Number"], "4242424242424242");
        // Aucun document ne doit fuiter
        assert!(result.iter().all(|v| v["type"] != "document"));
    }
}
```

- [ ] **Step 2 : Vérifier l'échec**

Run: `cd src-tauri && cargo test commands::browser`
Attendu : erreur de compilation — `handle_get_identities` n'existe pas.

- [ ] **Step 3 : Implémenter `handle_get_identities` dans `browser.rs`** (sous `handle_get_logins`)

```rust
/// Identités et cartes exposées au navigateur pour le remplissage de
/// formulaires (action get-identities). Les documents sont volontairement
/// exclus : aucun usage autofill, surface d'exposition en moins.
pub fn handle_get_identities(db: &crate::kdbx::xml::KeePassFile) -> Vec<serde_json::Value> {
    collect_all_entries(&db.root.group)
        .iter()
        .filter_map(|e| {
            let item_type = e.get_field("MyPass_Type")?;
            if item_type != "identity" && item_type != "card" {
                return None;
            }
            let fields: serde_json::Map<String, serde_json::Value> = e
                .strings
                .iter()
                .filter(|s| s.key.starts_with("ID_") || s.key.starts_with("CC_"))
                .map(|s| (s.key.clone(), serde_json::Value::String(s.value.content.clone())))
                .collect();
            Some(serde_json::json!({
                "uuid": e.uuid,
                "title": e.title(),
                "type": item_type,
                "fields": fields,
            }))
        })
        .collect()
}
```

(Si `collect_all_entries` renvoie des valeurs et non des références, adapter `.iter()` en conséquence — le test le dira.)

- [ ] **Step 4 : Brancher l'action dans `native_messaging.rs`**

Dans le `match req.action.as_str()`, après `"get-totp" => ...` :

```rust
        "get-identities" => handle_get_identities_action(req, session, db_state),
```

Ajouter le handler (près de `handle_get_logins`), modelé sur lui :

```rust
fn handle_get_identities_action(
    req: &NativeRequest,
    session: &SessionState,
    db_state: &Arc<Mutex<DbState>>,
) -> NativeResponse {
    // Valide la session/nonce comme les autres actions chiffrées ; le
    // contenu du message est vide (pas de filtre URL : les identités ne
    // sont pas liées à un site).
    if let Err(e) = decrypt_message(req, session) {
        return error_response(req, 1, &format!("Decrypt error: {e}"));
    }

    let db = match db_state.lock() {
        Ok(db) => db,
        Err(_) => return error_response(req, 3, "Database lock error"),
    };

    let kf = match db.keepass_file.as_ref() {
        Some(kf) => kf,
        None => return error_response(req, 2, "No open database"),
    };

    let identities = browser::handle_get_identities(kf);

    NativeResponse {
        action: Some("get-identities".to_string()),
        message: None,
        nonce: None,
        client_id: Some(req.client_id.clone()),
        error: None,
        error_code: None,
        version: None,
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("identities".to_string(), serde_json::json!(identities));
            map
        },
    }
}
```

Mettre à jour le commentaire de doc en tête de fichier (liste des actions) : ajouter `/// - get-identities: Retrieve identities and cards for form filling`.

- [ ] **Step 5 : Vérifier**

Run: `cd src-tauri && cargo test` — attendu : tous les tests PASS (y compris Task 2).
Run: `cd src-tauri && cargo clippy` — attendu : aucun nouveau warning.

---

### Task 8 : Extension — relais background `get_identities`

**Files:**
- Modify: `keepassxc-browser/keepassxc-browser/background/keepass.js`
- Modify: `keepassxc-browser/keepassxc-browser/background/event.js`

**Interfaces:**
- Consumes: `keepassClient.sendMessage`, `keepass.testAssociation`, `kpActions` (existants) ; action protocole `get-identities` (Task 7).
- Produces: message runtime `'get_identities'` → `Promise<Array<{uuid, title, type, fields}>>` (consommé par la tâche 9 via `sendMessage('get_identities')`).

- [ ] **Step 1 : Déclarer l'action**

Dans `keepass.js`, objet `kpActions` (~ligne 32), ajouter :

```js
    GET_IDENTITIES: 'get-identities',
```

- [ ] **Step 2 : Implémenter `keepass.getIdentities`** (sous `keepass.getTotp`, ~ligne 592)

```js
// MyPass-specific: fetch identities and bank cards for form filling.
// Not part of the upstream KeePassXC protocol.
keepass.getIdentities = async function(tab, args = []) {
    const taResponse = await keepass.testAssociation(tab, [ false ]);
    if (!taResponse || !keepass.isConnected) {
        return [];
    }

    const kpAction = kpActions.GET_IDENTITIES;
    const nonce = keepassClient.getNonce();

    const messageData = {
        action: kpAction
    };

    try {
        const response = await keepassClient.sendMessage(kpAction, tab, messageData, nonce);
        if (response) {
            keepass.updateLastUsed(keepass.databaseHash);
            return response.identities ?? [];
        }

        return [];
    } catch (err) {
        logError(`getIdentities failed: ${err}`);
        return [];
    }
};
```

- [ ] **Step 3 : Enregistrer le message handler**

Dans `event.js`, objet `kpxcEvent.messageHandlers`, après `'get_features_list': ...` (ordre alphabétique) :

```js
    'get_identities': keepass.getIdentities,
```

- [ ] **Step 4 : Vérifier**

Recharger l'extension non-empaquetée (chrome://extensions → Recharger). Ouvrir la console du service worker et exécuter :

```js
browser.tabs.query({ active: true }).then(([tab]) => keepass.getIdentities(tab).then(console.log));
```

Attendu (MyPass ouvert + coffre déverrouillé + une carte créée en Task 5) : tableau avec `{uuid, title, type: "card", fields: {...}}`. MyPass fermé : `[]` sans exception.

---

### Task 9 : Extension — détection de champs, menu et remplissage

**Files:**
- Create: `keepassxc-browser/keepassxc-browser/content/identity-fields.js`
- Modify: `keepassxc-browser/keepassxc-browser/manifest.json`
- Create: `test-fixtures/identity-card-forms.html`

**Interfaces:**
- Consumes: globals des content scripts chargés avant lui : `Icon`, `getIconClass`, `kpxcUI`, `kpxcIcons`, `kpxcFields`, `kpxcFill.setValue`, `Autocomplete`, `DatabaseState`, `kpxc`, `sendMessage`, `showErrorNotification`, `String.prototype/Element` helpers (`getLowerCaseAttribute`), `Pixels` ; message `'get_identities'` (Task 8).
- Produces: rien (feuille).

- [ ] **Step 1 : Créer `content/identity-fields.js`**

```js
'use strict';

// MyPass : remplissage d'identités et de cartes bancaires (absent de
// l'upstream keepassxc-browser). Détecte les champs via l'attribut
// autocomplete, puis par heuristiques name/id/placeholder/label (FR+EN).

const kpxcIdentityFill = {};
kpxcIdentityFill.icons = [];

// Jeton autocomplete -> clé MyPass ('@x' = valeur composée, null = ignoré)
kpxcIdentityFill.autocompleteMap = {
    'cc-number': 'CC_Number',
    'cc-name': 'CC_Holder',
    'cc-csc': 'CC_CVC',
    'cc-exp': '@cc-exp',
    'cc-exp-month': 'CC_ExpMonth',
    'cc-exp-year': 'CC_ExpYear',
    'given-name': 'ID_FirstName',
    'family-name': 'ID_LastName',
    'name': '@full-name',
    'email': 'ID_Email',
    'tel': 'ID_Phone',
    'tel-national': 'ID_Phone',
    'street-address': 'ID_Address',
    'address-line1': 'ID_Address',
    'address-level2': 'ID_City',
    'postal-code': 'ID_PostalCode',
    'country': 'ID_Country',
    'country-name': 'ID_Country',
    'bday': 'ID_BirthDate',
    'organization': 'ID_Company'
};

// Heuristiques de repli, testées dans l'ordre (carte avant identité :
// « nom sur la carte » doit matcher CC_Holder, pas ID_LastName).
kpxcIdentityFill.heuristics = [
    [ /card.?number|cardnum|num[eé]ro.?(de.?)?carte|num.?cb|\bpan\b/i, 'CC_Number' ],
    [ /cvc|cvv|csc|cryptogramme|security.?code/i, 'CC_CVC' ],
    [ /exp.*month|mois.*exp/i, 'CC_ExpMonth' ],
    [ /exp.*year|ann[eé]e.*exp/i, 'CC_ExpYear' ],
    [ /expir/i, '@cc-exp' ],
    [ /card.?holder|titulaire|name.?on.?card|nom.*carte/i, 'CC_Holder' ],
    [ /first.?name|pr[eé]nom|given/i, 'ID_FirstName' ],
    [ /last.?name|surname|family|nom.?de.?famille|^nom$/i, 'ID_LastName' ],
    [ /e.?mail|courriel/i, 'ID_Email' ],
    [ /phone|mobile|t[eé]l[eé]phone|portable/i, 'ID_Phone' ],
    [ /postal.?code|\bzip\b|code.?postal/i, 'ID_PostalCode' ],
    [ /address|adresse|\brue\b|street/i, 'ID_Address' ],
    [ /city|ville|town/i, 'ID_City' ],
    [ /country|pays/i, 'ID_Country' ],
    [ /birth|naissance/i, 'ID_BirthDate' ],
    [ /company|soci[eé]t[eé]|organi[sz]ation/i, 'ID_Company' ]
];

kpxcIdentityFill.labelText = function(field) {
    return field.labels?.[0]?.textContent ?? '';
};

// Clé MyPass d'un champ, ou null s'il ne nous concerne pas
kpxcIdentityFill.fieldKey = function(field) {
    if (field.type === 'password' || field.type === 'hidden' || field.readOnly) {
        return null;
    }

    const auto = field.getLowerCaseAttribute('autocomplete');
    if (auto) {
        for (const token of auto.split(' ')) {
            if (token in kpxcIdentityFill.autocompleteMap) {
                return kpxcIdentityFill.autocompleteMap[token];
            }
        }
    }

    const haystack = [ field.name, field.id, field.placeholder, kpxcIdentityFill.labelText(field) ]
        .filter(Boolean).join(' ');
    for (const [ regex, key ] of kpxcIdentityFill.heuristics) {
        if (regex.test(haystack)) {
            return key;
        }
    }

    return null;
};

// Scanne la page, pose une icône par formulaire identité/carte détecté
kpxcIdentityFill.scan = function() {
    const byRoot = new Map(); // form (ou document) -> [{ el, key }]
    for (const el of document.querySelectorAll('input, select')) {
        if (el.offsetParent === null) {
            continue; // champ invisible
        }
        const key = kpxcIdentityFill.fieldKey(el);
        if (!key) {
            continue;
        }
        const root = el.form || document;
        if (!byRoot.has(root)) {
            byRoot.set(root, []);
        }
        byRoot.get(root).push({ el: el, key: key });
    }

    for (const fields of byRoot.values()) {
        const isPayment = fields.some(f => f.key === 'CC_Number' || f.key === 'CC_CVC' || f.key === '@cc-exp');
        const identityCount = fields.filter(f => f.key.startsWith('ID_') || f.key === '@full-name').length;
        // Un formulaire de login avec juste un email n'est pas un formulaire d'identité
        if (!isPayment && identityCount < 2) {
            continue;
        }

        const anchor = fields[0].el;
        if (anchor.getAttribute('kpxc-identity-icon') === '1') {
            continue;
        }
        anchor.setAttribute('kpxc-identity-icon', '1');

        const formEntry = { fields: fields, type: isPayment ? 'card' : 'identity' };
        kpxcIdentityFill.icons.push(new IdentityFieldIcon(anchor, formEntry, kpxc.databaseState));
    }
};

kpxcIdentityFill.showMenu = async function(field, formEntry) {
    const all = await sendMessage('get_identities');
    const items = (all || []).filter(i => i.type === formEntry.type);
    if (items.length === 0) {
        showErrorNotification(formEntry.type === 'card'
            ? 'MyPass : aucune carte dans le coffre'
            : 'MyPass : aucune identité dans le coffre', 'warning');
        return;
    }

    if (items.length === 1) {
        await kpxcIdentityFill.fill(formEntry, items[0]);
        return;
    }

    kpxcIdentityAutocomplete.identityItems = items;
    kpxcIdentityAutocomplete.formEntry = formEntry;
    kpxcIdentityAutocomplete.elements = items.map(i => ({
        title: i.title,
        value: i.type === 'card'
            ? ('•••• ' + (i.fields.CC_Number || '').slice(-4))
            : (i.fields.ID_Email || [ i.fields.ID_FirstName, i.fields.ID_LastName ].filter(Boolean).join(' ')),
        group: '',
        uuid: i.uuid
    }));
    kpxcIdentityAutocomplete.showList(field, true);
};

// Valeur à écrire pour une clé, gère les valeurs composées
kpxcIdentityFill.resolveValue = function(key, fields) {
    if (key === '@cc-exp') {
        const month = (fields.CC_ExpMonth || '').padStart(2, '0');
        const year = (fields.CC_ExpYear || '').slice(-2);
        return (month !== '00' && year) ? `${month}/${year}` : '';
    }
    if (key === '@full-name') {
        return [ fields.ID_FirstName, fields.ID_LastName ].filter(Boolean).join(' ');
    }
    return fields[key] || '';
};

kpxcIdentityFill.fill = async function(formEntry, item) {
    for (const { el, key } of formEntry.fields) {
        const value = kpxcIdentityFill.resolveValue(key, item.fields);
        if (!value) {
            continue;
        }
        if (el instanceof HTMLSelectElement) {
            kpxcIdentityFill.fillSelect(el, key, value);
        } else {
            await kpxcFill.setValue(el, value);
        }
    }
};

// Les expirations sont souvent des <select> : "1"/"01"/"2027"...
kpxcIdentityFill.fillSelect = function(el, key, value) {
    const candidates = [ value ];
    if (key === 'CC_ExpMonth') {
        candidates.push(String(Number(value)), value.padStart(2, '0'));
    } else if (key === 'CC_ExpYear') {
        candidates.push(value.length === 2 ? '20' + value : value.slice(-2));
    }
    const option = Array.from(el.options).find(o =>
        candidates.includes(o.value.trim()) || candidates.includes(o.textContent.trim()));
    if (option) {
        el.value = option.value;
        el.dispatchEvent(new Event('change', { bubbles: true }));
    }
};

class IdentityFieldIcon extends Icon {
    constructor(field, formEntry, databaseState = DatabaseState.DISCONNECTED) {
        super(field, databaseState);
        this.formEntry = formEntry;
        this.createIcon(field);
        this.inputField = field;
        kpxcIcons.monitorIconPosition(this);
    }
}

// Même construction que TOTPFieldIcon, en réutilisant le style de
// l'icône username (pas de nouveau CSS).
IdentityFieldIcon.prototype.createIcon = function(field) {
    const className = getIconClass('kpxc-username-icon');
    const size = this.calculateIconSize(field);
    const formEntry = this.formEntry;

    const icon = kpxcUI.createElement('div', 'kpxc kpxc-username-icon ' + className,
        {
            'title': 'MyPass : remplir avec une identité / carte enregistrée',
            'size': size,
            'popover': 'manual'
        });

    if (kpxcFields.popoverSupported) {
        icon.style.margin = 0;
    } else {
        icon.style.zIndex = '10000000';
    }
    icon.style.width = Pixels(size);
    icon.style.height = Pixels(size);

    if (this.databaseState === DatabaseState.DISCONNECTED || this.databaseState === DatabaseState.LOCKED) {
        icon.style.filter = 'saturate(0%)';
    } else {
        icon.style.filter = 'saturate(100%)';
    }

    icon.addEventListener('click', async function(e) {
        if (!e.isTrusted) {
            return;
        }
        e.stopPropagation();
        await kpxcIdentityFill.showMenu(field, formEntry);
    });

    icon.addEventListener('mousedown', ev => ev.stopPropagation());
    icon.addEventListener('mouseup', ev => ev.stopPropagation());

    kpxcIcons.setIconPosition(icon, field, this.rtl);
    this.icon = icon;
    this.createWrapper('css/username.css');
    if (kpxcFields.popoverSupported) {
        icon.showPopover();
    }
};

class IdentityAutocomplete extends Autocomplete {}

IdentityAutocomplete.prototype.itemClick = async function(e, input, uuid) {
    if (!e.isTrusted) {
        return;
    }
    const item = this.identityItems.find(i => i.uuid === uuid);
    if (item) {
        await kpxcIdentityFill.fill(this.formEntry, item);
    }
    this.closeList();
};

IdentityAutocomplete.prototype.itemEnter = async function(index, item) {
    const uuid = item?.getAttribute('uuid');
    const found = this.identityItems.find(i => i.uuid === uuid);
    if (found) {
        await kpxcIdentityFill.fill(this.formEntry, found);
    }
};

const kpxcIdentityAutocomplete = new IdentityAutocomplete();

// Scan initial + re-scan sur mutations (checkouts SPA), avec debounce.
(function initIdentityFill() {
    let timer;
    const rescan = () => {
        clearTimeout(timer);
        timer = setTimeout(() => kpxcIdentityFill.scan(), 500);
    };

    if (document.readyState !== 'loading') {
        rescan();
    } else {
        document.addEventListener('DOMContentLoaded', rescan);
    }

    // ponytail: MutationObserver global débounce à 500 ms ; si un site
    // très dynamique pose problème, brancher sur observer-helper.js
    new MutationObserver(rescan).observe(document.documentElement, { childList: true, subtree: true });
})();
```

- [ ] **Step 2 : Enregistrer le script dans `manifest.json`**

Dans le premier bloc `content_scripts`, ajouter à la fin du tableau `js` (après `"content/username-field.js"`) :

```json
                "content/identity-fields.js"
```

- [ ] **Step 3 : Créer la page de test `test-fixtures/identity-card-forms.html`**

```html
<!DOCTYPE html>
<html lang="fr">
<head><meta charset="utf-8"><title>MyPass — test autofill identité/carte</title>
<style>body{font-family:sans-serif;max-width:640px;margin:2rem auto}fieldset{margin-bottom:2rem;padding:1rem}label{display:block;margin-top:.5rem}input,select{width:100%;padding:.4rem}</style>
</head>
<body>
<h1>Formulaires de test MyPass</h1>

<fieldset>
<legend>Paiement (attributs autocomplete standard)</legend>
<form>
  <label>Titulaire <input autocomplete="cc-name" name="ccname"></label>
  <label>Numéro de carte <input autocomplete="cc-number" name="cardnumber" inputmode="numeric"></label>
  <label>Mois <select autocomplete="cc-exp-month" name="ccmonth"><option value=""></option><option value="1">01</option><option value="2">02</option><option value="3">03</option><option value="7">07</option><option value="12">12</option></select></label>
  <label>Année <select autocomplete="cc-exp-year" name="ccyear"><option value=""></option><option>2026</option><option>2027</option><option>2028</option><option>2029</option></select></label>
  <label>CVC <input autocomplete="cc-csc" name="cvc" inputmode="numeric" maxlength="4"></label>
</form>
</fieldset>

<fieldset>
<legend>Inscription (sans autocomplete — heuristiques FR)</legend>
<form>
  <label>Prénom <input name="prenom"></label>
  <label>Nom <input name="nom"></label>
  <label>Email <input name="courriel" type="email"></label>
  <label>Téléphone <input name="telephone" type="tel"></label>
  <label>Adresse <input name="adresse"></label>
  <label>Code postal <input name="code_postal"></label>
  <label>Ville <input name="ville"></label>
</form>
</fieldset>
</body>
</html>
```

- [ ] **Step 4 : Vérification E2E manuelle (bout en bout)**

1. `npm run tauri dev` — ouvrir le coffre de test (dans `src-tauri\`), vérifier que l'intégration navigateur est activée (sinon : page Browser de l'app). Créer si besoin 2 cartes et 1 identité (Task 5).
2. Recharger l'extension non-empaquetée (`chrome://extensions`).
3. Servir la page de test : `npx serve test-fixtures` (ou tout serveur statique) et ouvrir `http://localhost:3000/identity-card-forms.html`.
4. Formulaire paiement : icône dans le champ « Titulaire » → clic → menu listant les 2 cartes → sélection → titulaire, numéro, mois (select), année (select), CVC remplis.
5. Formulaire inscription : icône présente (heuristiques) → clic → l'identité remplit prénom, nom, email, téléphone, adresse, code postal, ville.
6. Coffre verrouillé : clic sur l'icône → notification « aucune carte/identité » ou rien, pas d'exception dans la console (F12).
7. Vérifier dans la console qu'aucune erreur `identity-fields.js` n'apparaît sur un site ordinaire (ex. github.com) sans formulaire identité/carte.

---

## Vérification finale (après toutes les tâches)

- `cd src-tauri && cargo test && cargo clippy` — tout passe.
- `npm run build && npm run lint` — tout passe.
- Parcours complet dans `npm run tauri dev` : créer identité + carte + document → copie un clic → sauvegarder, rouvrir le coffre → tout est là → autofill des deux formulaires de test depuis le navigateur.
- Ouvrir le coffre dans KeePassXC (vendorisé ou installé) : les éléments apparaissent comme entrées avec attributs personnalisés, `CC_Number`/`CC_CVC`/`DOC_Number` protégés.

## Reporté (assumé)

- Scans de documents en pièce jointe : nécessite le pool binaire KDBX4 dans `reader.rs`/`writer.rs` (v1.1, voir spec).
- `CC_Type` stocké : dérivé à l'affichage par `cardBrand()`, jamais persisté.
- Rendu « carte bancaire » stylisé dans EntryDetail (mentionné dans la spec) : v1 affiche des lignes de champs standard ; le visuel carte est cosmétique, à ajouter plus tard si souhaité.
- Bouton Supprimer d'`EntryDetail` : n'était pas branché avant cette feature, hors périmètre.
- Champs iframe cross-origin (Stripe Elements) : le content script tourne aussi dans les iframes (`all_frames: true`), donc les champs y sont détectés — mais chaque iframe demande son propre clic d'icône.
