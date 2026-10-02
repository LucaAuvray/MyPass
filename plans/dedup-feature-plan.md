# Feature Plan — Déduplication à l'import

## 1. Objective

Éviter les doublons lors de l'import de mots de passe. Après parsing du fichier, détecter les entrées en conflit (même site + login) et laisser l'utilisateur choisir laquelle conserver via une popup de comparaison. Une seule entrée par groupe est conservée, les autres sont supprimées.

## 2. Confirmed Requirements

| # | Requirement | Details |
|---|-------------|---------|
| R1 | Matching URL + username | Si l'un est vide, matcher sur l'autre (fallback URL seule ou username seul) |
| R2 | Doublons internes | Si le fichier contient 3× le même site+login, les 3 sont montrés |
| R3 | Pré-sélection | L'entrée importée est pré-sélectionnée (données potentiellement + récentes) |
| R4 | Données affichées | Tout : titre, username, password avec révélation, URL, notes, dates, tags, custom fields, TOTP |
| R5 | Jamais de doublons | 1 seule entrée conservée par groupe, pas d'option « garder les deux » |
| R6 | Annulation | Si l'utilisateur ferme la popup sans confirmer → import annulé |

## 3. Non-goals

- Déduplication rétroactive sur le coffre existant (feature future : « Find duplicates » dans Security)
- Fusion intelligente des champs (ex: garder le password de A + les notes de B)
- Détection par similarité floue (fuzzy matching sur les URLs)
- Déduplication cross-format (ex: comparer 1Password vs Bitwarden dans le même import)

## 4. Architecture Decision

**Frontend-heavy approach** : toute la logique de parsing CSV et de détection de doublons est côté frontend (TypeScript). Le backend Rust ne fait que lire le fichier et insérer les entrées résolues.

**Rationale** :
- Le parsing CSV existe déjà côté frontend (mode mock)
- La déduplication est une logique UI (affichage, comparaison, sélection)
- Moins de code Rust à écrire/maintenir
- Plus facile à tester en mode browser

## 5. Flow

```
┌──────────┐    ┌──────────┐    ┌──────────────┐    ┌───────────┐    ┌──────┐
│  FORMAT  │───▶│ PARSING  │───▶│    DEDUP      │───▶│ IMPORTING │───▶│ DONE │
│ sélection│    │ lecture  │    │ comparaison   │    │ commit    │    │résult│
│ fichier  │    │ + détect.│    │ + résolution  │    │ backend   │    │      │
└──────────┘    └──────────┘    └──────────────┘    └───────────┘    └──────┘
                     │                                  ▲
                     │ (si 0 doublons)                  │
                     └──────────────────────────────────┘
```

### Step details:

**FORMAT → PARSING** : L'utilisateur choisit le format, sélectionne le fichier. Le contenu est lu (via Tauri `read_file_content` ou File API en browser). Le CSV/JSON est parsé côté frontend. Les groupes de doublons sont calculés.

**PARSING → DEDUP** : Si `duplicateGroups.length > 0`, afficher [`DedupDialog`](src/components/import-export/DedupDialog.tsx). Sinon, passer directement à IMPORTING.

**DEDUP → IMPORTING** : L'utilisateur a sélectionné une entrée par groupe. Les entrées résolues (non-dupliquées + sélectionnées) sont envoyées au backend via `import_entries`.

**IMPORTING → DONE** : Afficher le résultat (X entrées importées, Y doublons supprimés, Z erreurs).

## 6. Files Involved

### New files

| File | Purpose |
|------|---------|
| [`src/lib/dedup.ts`](src/lib/dedup.ts) | Logique pure : `computeDuplicateGroups()`, `normalizeForMatching()`, `parseCsvToEntries()` |
| [`src/components/import-export/DedupDialog.tsx`](src/components/import-export/DedupDialog.tsx) | UI de comparaison : header, groupes, cartes côte-à-côte, radio selection |

### Modified files

| File | Changes |
|------|---------|
| [`src/types/import.ts`](src/types/import.ts) | Ajouter `DuplicateGroup`, `DuplicateEntry`, `ResolvedEntry`, `ParsedEntry` |
| [`src/components/import-export/ImportWizard.tsx`](src/components/import-export/ImportWizard.tsx:32) | Ajouter steps `"parsing"` et `"dedup"`, intégrer `DedupDialog`, appeler `computeDuplicateGroups` |
| [`src/lib/tauri.ts`](src/lib/tauri.ts:48) | Mock: ajouter `read_file_content`, `import_entries`, `get_entries_for_dedup` |
| [`src-tauri/src/commands/import_export.rs`](src-tauri/src/commands/import_export.rs:1) | Ajouter `read_file_content`, `import_entries`, `get_entries_for_dedup` |
| [`src-tauri/src/commands/mod.rs`](src-tauri/src/commands/mod.rs) | Enregistrer les nouvelles commandes |
| [`src-tauri/src/lib.rs`](src-tauri/src/lib.rs:18) | Enregistrer les nouvelles commandes |
| [`src/i18n/fr.json`](src/i18n/fr.json) | Clés : `dedup.*` |
| [`src/i18n/en.json`](src/i18n/en.json) | Clés : `dedup.*` |

## 7. Data Model

```typescript
// src/types/import.ts — new types

/** A parsed entry from the import file, before any dedup. */
interface ParsedEntry {
  tempId: string;       // unique within the import batch
  title: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  tags: string[];
  customFields: Record<string, string>;
  totp: string;
}

/** One side of a duplicate comparison. */
interface DuplicateEntry {
  source: "import" | "vault";
  uuid?: string;        // only for vault entries
  tempId?: string;      // only for import entries
  title: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  tags: string[];
  customFields: Record<string, string>;
  totp: string;
  created: string;      // ISO 8601
  modified: string;     // ISO 8601
}

/** A group of entries that match on the same key. */
interface DuplicateGroup {
  id: string;           // unique group id (e.g. "dup-0", "dup-1")
  matchKey: string;     // display label: "google.com — user@gmail.com"
  matchType: "url+username" | "url" | "username";
  entries: DuplicateEntry[];
  selectedIndex: number; // index in entries[] of the chosen one (default: first import entry)
}

/** An entry resolved after dedup, ready for import. */
interface ResolvedEntry {
  title: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  tags: string[];
  customFields: Record<string, string>;
  totp: string;
}

/** Result of the dedup resolution. */
interface DedupResolution {
  groups: DuplicateGroup[];
  resolvedEntries: ResolvedEntry[]; // entries to actually import
  totalKept: number;
  totalDiscarded: number;
}
```

## 8. Dedup Algorithm (`src/lib/dedup.ts`)

```
function computeDuplicateGroups(
  importEntries: ParsedEntry[],
  vaultEntries: DuplicateEntry[]
): DuplicateGroup[]

1. Build a Map<string, { import: ParsedEntry[], vault: DuplicateEntry[] }>
   Key format: normalize(url) + "::" + username.toLowerCase().trim()
   - If url is empty: key = "__empty__::" + username.toLowerCase().trim()
   - If username is empty: key = normalize(url) + "::__empty__"

2. normalize(url): extract hostname, lowercase, remove "www." prefix
   - "https://www.Google.com/mail" → "google.com"
   - "" → ""
   - "not a url" → "not a url".toLowerCase()

3. For each import entry, add to map under its key
4. For each vault entry, add to map under its key

5. For each map entry where the total (import + vault) >= 2:
   - Create a DuplicateGroup
   - Order entries: import entries first (so selectedIndex=0 picks import)
   - selectedIndex = 0 (first import entry, or first vault if no import)
   - matchKey = best display label from the entries

6. Return groups sorted by matchKey
```

## 9. DedupDialog UI Design

```
┌──────────────────────────────────────────────────────────┐
│  🔍 Doublons détectés                            [✕]    │
│                                                          │
│  3 groupes de doublons trouvés. Choisissez l'entrée à    │
│  conserver pour chaque groupe. Les autres seront         │
│  supprimées.                                             │
│                                                          │
│  ⚡ Tout sélectionner les imports                         │
│                                                          │
│  ┌─ google.com — user@gmail.com ─────────────────────┐   │
│  │                                                    │   │
│  │  ◉ Import                ○ Existant (coffre)      │   │
│  │  ┌─────────────────┐    ┌─────────────────┐       │   │
│  │  │ Titre  Google   │    │ Titre  Google   │       │   │
│  │  │ User   user@... │    │ User   user@... │       │   │
│  │  │ MDP    •••••••• 👁   │ MDP    •••••••• 👁      │   │
│  │  │ URL    google.. │    │ URL    google.. │       │   │
│  │  │ Notes  "Notes"  │    │ Notes  —        │       │   │
│  │  │ Tags   email    │    │ Tags   email    │       │   │
│  │  │ TOTP   Oui      │    │ TOTP   Non      │       │   │
│  │  │ Créé   2024-03  │    │ Créé   2023-01  │       │   │
│  │  └─────────────────┘    └─────────────────┘       │   │
│  └────────────────────────────────────────────────────┘   │
│                                                          │
│  ┌─ github.com — dev ─────────────────────────────────┐   │
│  │  ◉ Import                ○ Existant (coffre)       │   │
│  │  (similar cards)                                    │   │
│  └────────────────────────────────────────────────────┘   │
│                                                          │
│  ┌─ (interne) twitter.com — @handle ──────────────────┐   │
│  │  ◉ Entrée 1    ○ Entrée 2    ○ Entrée 3            │   │
│  │  (3 entrées identiques dans le fichier d'import)     │   │
│  └────────────────────────────────────────────────────┘   │
│                                                          │
│  ────────────────────────────────────────────────────    │
│  📊 3 entrées conservées · 5 doublons supprimés          │
│                                                          │
│            [Annuler l'import]  [Confirmer et importer]   │
└──────────────────────────────────────────────────────────┘
```

### Responsive behavior:
- Desktop (≥768px) : cartes côte-à-côte (max 3 par ligne si >2 entrées)
- Mobile (<768px) : cartes empilées verticalement, swipe ou scroll horizontal

### States:
- **Loading** : "Analyse des doublons..." avec spinner
- **Empty** : Pas de doublons → skip automatique vers IMPORTING
- **Error** : "Erreur lors de l'analyse" → retour à FORMAT
- **Resolved** : Tous les groupes ont une sélection → bouton "Confirmer" actif
- **Unresolved** : Au moins un groupe sans sélection → bouton "Confirmer" désactivé

## 10. Implementation Details

### 10.1 `src/lib/dedup.ts`

Pure functions, no React dependency:
- `normalizeUrl(url: string): string` — extrait le hostname
- `makeMatchKey(url: string, username: string): string` — produit la clé de matching
- `parseCsvToEntries(content: string, format: ImportFormat): ParsedEntry[]` — parse CSV/JSON
- `computeDuplicateGroups(importEntries: ParsedEntry[], vaultEntries: DuplicateEntry[]): DuplicateGroup[]`
- `resolveDuplicates(groups: DuplicateGroup[], nonDuplicates: ParsedEntry[]): DedupResolution`

### 10.2 `DedupDialog.tsx`

Props:
```typescript
interface DedupDialogProps {
  open: boolean;
  groups: DuplicateGroup[];
  onConfirm: (resolution: DedupResolution) => void;
  onCancel: () => void;
}
```

Internal state:
- `groups` avec `selectedIndex` modifiable par l'utilisateur
- `showPassword: Record<string, boolean>` — quel mot de passe est révélé

Rendering:
- Pour chaque `DuplicateGroup`, render `DuplicateGroupCard`
- `DuplicateGroupCard` : header (matchKey + badge source), radio buttons, entry cards

### 10.3 `ImportWizard.tsx` changes

Nouveau step enum : `"format" | "parsing" | "dedup" | "importing" | "done"`

Nouvel état :
```typescript
const [duplicateGroups, setDuplicateGroups] = useState<DuplicateGroup[]>([]);
const [parsedEntries, setParsedEntries] = useState<ParsedEntry[]>([]);
```

Dans `handleFilePicked` (ou `handleSelectAndImport` pour Tauri) :
1. Lire le contenu du fichier
2. Parser → `ParsedEntry[]`
3. Récupérer les entrées existantes du coffre → `DuplicateEntry[]`
4. `computeDuplicateGroups(parsed, vault)` → `DuplicateGroup[]`
5. Si `groups.length > 0` → `setStep("dedup")`, `setDuplicateGroups(groups)`
6. Sinon → `executeImport(resolvedEntries)` directement

Nouvelle fonction `handleDedupConfirm(resolution: DedupResolution)` :
1. `setStep("importing")`
2. Appeler `tauriCommand("import_entries", { entries: resolution.resolvedEntries })`
3. → `setStep("done")`

### 10.4 Backend — New Tauri Commands

```rust
// Lecture du fichier pour parsing côté frontend
#[tauri::command]
async fn read_file_content(path: String) -> Result<String, String>;

// Récupérer les entrées existantes pour comparaison (sans mot de passe ?)
// → Le frontend a besoin du mot de passe pour l'afficher dans la comparaison
#[tauri::command]
async fn get_entries_for_dedup(state: State<'_, Mutex<DbState>>) -> Result<Vec<DedupEntry>, String>;

// Importer les entrées résolues
#[tauri::command]
async fn import_entries(
    state: State<'_, Mutex<DbState>>,
    entries: Vec<ResolvedEntry>,
) -> Result<ImportResult, String>;
```

### 10.5 Mock Backend (`src/lib/tauri.ts`)

- `read_file_content` : retourne une string mockée (déjà lu côté frontend via File API)
- `get_entries_for_dedup` : retourne `mockStore.entries` formatées en `DuplicateEntry`
- `import_entries` : ajoute les entrées dans `mockStore.entries`, retourne `ImportResult`

## 11. Edge Cases

| # | Edge Case | Handling |
|---|-----------|----------|
| E1 | 0 doublons détectés | Skip automatique → IMPORTING, pas de DedupDialog |
| E2 | URL vide dans les deux entrées | Matcher sur username seul |
| E3 | Username vide dans les deux entrées | Matcher sur URL seule |
| E4 | URL ET username vides dans les deux | Ne pas considérer comme doublon (skip) |
| E5 | 5 entrées pour le même site+login | Afficher les 5 cartes, scroll horizontal si + de 3 |
| E6 | L'utilisateur ferme le dialogue sans confirmer | Retour à FORMAT, état réinitialisé |
| E7 | Le fichier contient des caractères spéciaux | Géré par le parser CSV (guillemets, échappements) |
| E8 | Entrée existante supprimée entre-temps | Rafraîchir `get_entries_for_dedup` juste avant l'import |
| E9 | Gros fichier (>500 entrées) | Pagination dans le DedupDialog ? Pour le MVP, tout afficher avec scroll |
| E10 | Tous les groupes résolus → Confirmer | Bouton actif seulement si `groups.every(g => g.selectedIndex >= 0)` |

## 12. i18n Keys

```json
// fr.json
{
  "dedup": {
    "title": "Doublons détectés",
    "description": "{{count}} groupes de doublons trouvés. Choisissez l'entrée à conserver pour chaque groupe. Les autres seront supprimées.",
    "selectAllImports": "Tout sélectionner les imports",
    "selectAllExisting": "Tout sélectionner l'existant",
    "sourceImport": "Import",
    "sourceVault": "Coffre",
    "sourceInternal": "Interne",
    "internalLabel": "{{count}} entrées identiques dans le fichier",
    "fieldTitle": "Titre",
    "fieldUsername": "Identifiant",
    "fieldPassword": "Mot de passe",
    "fieldUrl": "URL",
    "fieldNotes": "Notes",
    "fieldTags": "Étiquettes",
    "fieldTotp": "TOTP",
    "fieldCreated": "Créé",
    "fieldModified": "Modifié",
    "fieldCustomFields": "Champs personnalisés",
    "revealPassword": "Afficher le mot de passe",
    "noNotes": "—",
    "noTags": "—",
    "noTotp": "Non",
    "hasTotp": "Oui",
    "summary": "{{kept}} entrées conservées · {{discarded}} doublons supprimés",
    "cancel": "Annuler l'import",
    "confirm": "Confirmer et importer",
    "analyzing": "Analyse des doublons..."
  }
}
```

## 13. Step-by-Step Todo List

1. **Ajouter les types** dans `src/types/import.ts` : `ParsedEntry`, `DuplicateEntry`, `DuplicateGroup`, `ResolvedEntry`, `DedupResolution`
2. **Créer `src/lib/dedup.ts`** : `normalizeUrl`, `makeMatchKey`, `parseCsvToEntries`, `computeDuplicateGroups`, `resolveDuplicates`
3. **Créer `src/components/import-export/DedupDialog.tsx`** : UI avec groupes, cartes, radio, password reveal, compteur, boutons confirmer/annuler
4. **Modifier `src/components/import-export/ImportWizard.tsx`** : ajouter steps `parsing`/`dedup`, intégrer `DedupDialog`, nouvelle fonction `handleDedupConfirm`
5. **Modifier `src/lib/tauri.ts`** : mock `read_file_content`, `get_entries_for_dedup`, `import_entries`
6. **Modifier `src-tauri/src/commands/import_export.rs`** : commandes `read_file_content`, `get_entries_for_dedup`, `import_entries`
7. **Enregistrer les commandes** dans `mod.rs` et `lib.rs`
8. **Ajouter les traductions** dans `fr.json` et `en.json`
9. **Corriger BUG #2** : fermeture du dialogue après "Terminé"
10. **Corriger BUG #3** : rafraîchissement de la liste après import (invalidate `get_entries` query)
11. **Tests Playwright** : test avec le CSV Google (24 entrées) + mock de doublons
