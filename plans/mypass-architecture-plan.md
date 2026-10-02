# MyPass — Plan d'Architecture Complet

> **Statut** : En attente de validation utilisateur
> **Date** : 2026-07-01

---

## 1. Résumé Exécutif

MyPass est un gestionnaire de mots de passe moderne inspiré de 1Password, construit avec **Tauri v2** (React + Tailwind + Rust), compatible avec le format **KDBX** (KeePass). Il réutilise l'extension navigateur **KeePassXC-Browser** existante pour l'intégration navigateur. L'application est une **PWA responsive** pour desktop et mobile.

---

## 2. Décisions Confirmées

| Décision | Choix |
|---|---|
| Architecture runtime | **Tauri v2** (React frontend + Rust backend) |
| Format base de données | **KDBX4** (compatibilité KeePassXC) |
| Intégration navigateur | Réutilisation de **KeePassXC-Browser** (Manifest V3) |
| Mobile | **PWA responsive** (apps natives React Native en phase 2) |
| UI Framework | React 19 + Tailwind CSS v4 + shadcn/ui |
| Périmètre MVP | Complet (toutes les features listées) |

---

## 3. Architecture Globale

```
┌────────────────────────────────────────────────────────┐
│                    MyPass (Tauri v2)                     │
│                                                         │
│  ┌──────────────────────┐  ┌─────────────────────────┐ │
│  │   Frontend (React)    │  │   Backend (Rust)         │ │
│  │                      │  │                          │ │
│  │  • shadcn/ui + TW4   │  │  • Crypto (KDBX4)       │ │
│  │  • React Router v7   │◄─┤  • Password Generator    │ │
│  │  • Zustand (state)   │  │  • TOTP Engine           │ │
│  │  • TanStack Query    │  │  • Import/Export         │ │
│  │  • PWA (Workbox)     │  │  • KeeShare protocol     │ │
│  │                      │  │  • File I/O              │ │
│  └──────────────────────┘  │  • Native Messaging      │ │
│                             │  • Passkeys crypto       │ │
│                             └─────────────────────────┘ │
│                                       │                  │
│                              ┌────────▼───────────┐     │
│                              │  IPC Bridge (JSON)  │     │
│                              │  Tauri Commands      │     │
│                              └──────────────────────┘     │
└────────────────────────────────────────────────────────┘
         │                              │
         ▼                              ▼
┌─────────────────┐    ┌──────────────────────────────┐
│  Fichier .kdbx  │    │  KeePassXC-Browser Extension  │
│  (local/cloud)  │    │  (Manifest V3, NaCl crypto)   │
└─────────────────┘    └──────────────────────────────┘
```

---

## 4. Stack Technique Détaillée

### 4.1 Frontend (React)

| Domaine | Technologie | Justification |
|---|---|---|
| Framework | React 19 | Dernière version stable |
| Bundler | Vite 6 | Rapide, natif Tauri |
| Routing | React Router v7 | SPA routing |
| UI Components | shadcn/ui (nova theme) | Composants accessibles, personnalisables |
| Styling | Tailwind CSS v4 | Utilitaire, rapide, responsive |
| State global | Zustand | Léger, simple, performant |
| Server state | TanStack Query v5 | Cache, refetch, mutations |
| Forms | React Hook Form + Zod | Validation performante |
| Icons | Lucide React | Large choix, léger |
| i18n | react-i18next | Multi-langues (FR/EN) |
| Drag & Drop | @dnd-kit/core | Réorganisation des groupes |
| PWA | vite-plugin-pwa (Workbox) | Offline, installable |
| Charts | Recharts (via shadcn Chart) | Dashboard de sécurité |
| Clipboard | @tauri-apps/plugin-clipboard-manager | Copie sécurisée |

### 4.2 Backend (Rust)

| Domaine | Crate | Justification |
|---|---|---|
| Runtime | Tauri v2 | Fenêtrage natif, IPC |
| Crypto KDBX | `kdbx-rs` (custom fork) | Lecture/écriture KDBX4 |
| Chiffrement | `aes-gcm`, `chacha20poly1305` | Chiffrement KDBX |
| Hash/KDF | `argon2`, `sha2` | Protection master password |
| Génération MDP | `rand` | CSPRNG |
| TOTP | `totp-rs` | Codes 2FA |
| Passkeys | `webauthn-rs` | FIDO2/WebAuthn |
| Import CSV | `csv` | Parsing CSV |
| Import JSON | `serde_json` | Parsing 1PUX/Bitwarden |
| Sérialisation | `serde` + `serde_json` | IPC bridge |
| File I/O | `tauri-plugin-fs` | Accès fichiers |
| SQLite | N/A (pas besoin — fichier KDBX) | — |
| Native Messaging | `tauri-plugin-shell` (custom) | Communication navigateur |
| Zeroization | `zeroize` | Nettoyage mémoire |
| Logging | `tracing` | Debug |

### 4.3 Extension Navigateur (héritée)

- KeePassXC-Browser Manifest V3 (JavaScript vanilla)
- Protocole NaCl (TweetNaCl.js) — chiffrement bout-en-bout
- Communication via Native Messaging (bridge Tauri)

---

## 5. Design System — MyPass

Basé sur le skill **frontend-design**, voici le système de design.

### 5.1 Direction Visuelle

MyPass s'inspire de 1Password mais avec sa propre identité. Le thème est **minimaliste, sécurisé, apaisant** — un coffre-fort numérique qui inspire confiance sans être froid.

**Signature** : Un effet de "glassmorphism" subtil sur les cards (fond semi-transparent + backdrop-blur) qui évoque la transparence et la sécurité. Les icônes des entrées utilisent un dégradé personnalisé par site web.

### 5.2 Palette

```
/* Light Mode */
--background: #FAFAF9          /* stone warm white */
--foreground: #1C1917          /* warm gray 900 */
--card: #FFFFFF
--card-foreground: #1C1917
--primary: #2563EB             /* blue 600 — confiance */
--primary-foreground: #FFFFFF
--secondary: #F5F3F0           /* warm gray 100 */
--secondary-foreground: #44403C
--muted: #F5F3F0
--muted-foreground: #78716C    /* warm gray 400 */
--accent: #7C3AED              /* violet 600 — énergie mesurée */
--accent-foreground: #FFFFFF
--destructive: #DC2626         /* red 600 */
--destructive-foreground: #FFFFFF
--border: #E7E5E4              /* warm gray 200 */
--input: #E7E5E4
--ring: #2563EB

/* Dark Mode */
--background: #0C0A09          /* warm gray 950 */
--foreground: #F5F3F0
--card: #1C1917DD              /* semi-transparent */
--card-foreground: #F5F3F0
--primary: #3B82F6             /* blue 500 */
--primary-foreground: #0C0A09
--secondary: #292524
--secondary-foreground: #D6D3D1
--muted: #292524
--muted-foreground: #A8A29E
--accent: #8B5CF6              /* violet 500 */
--accent-foreground: #FFFFFF
--destructive: #EF4444
--destructive-foreground: #FFFFFF
--border: #292524
--input: #292524
--ring: #3B82F6
```

### 5.3 Typographie

| Rôle | Police | Usage |
|---|---|---|
| Display | **DM Sans** (Google Fonts) | Titres, logo, hero sections |
| Body | **Inter** (Google Fonts) | Texte courant, labels, inputs |
| Mono | **JetBrains Mono** | Mots de passe, champs techniques |

### 5.4 Layout Concept

```
┌─────────────────────────────────────────────────┐
│ [Sidebar]  │  [Main Content]                    │
│            │                                     │
│ 🔒 MyPass  │  ┌─────────────────────────────┐   │
│ ─────────  │  │ 🔍 Search passwords...      │   │
│ 📁 Tous    │  └─────────────────────────────┘   │
│ 📁 Perso   │                                     │
│ 📁 Travail │  ┌──────────┐ ┌──────────┐         │
│ 📁 Banque  │  │ Google   │ │ GitHub   │  ...    │
│            │  │ user@... │ │ dev@...  │         │
│ ─────────  │  └──────────┘ └──────────┘         │
│ 🏷 Tags    │                                     │
│ ⚠ Faible  │                                     │
│ 🔑 Passkeys│                                     │
│ ─────────  │                                     │
│ ⚙ Settings │                                     │
└─────────────────────────────────────────────────┘
```

**Desktop** : Sidebar gauche + grille de cards à droite
**Mobile** : Bottom tab bar + liste verticale

### 5.5 Composants Clés shadcn/ui à Installer

```
sidebar, button, card, input, dialog, sheet, dropdown-menu,
tabs, badge, avatar, separator, skeleton, toast (sonner),
command, toggle-group, select, checkbox, switch, tooltip,
popover, alert-dialog, progress, collapsible, table, chart,
form (react-hook-form), field-group
```

---

## 6. Structure du Projet

```
mypass/
├── src/                          # Frontend React
│   ├── main.tsx                  # Entry point
│   ├── App.tsx                   # Root component + router
│   ├── index.css                 # Tailwind + design tokens
│   ├── components/
│   │   ├── ui/                   # shadcn/ui components
│   │   ├── layout/
│   │   │   ├── AppSidebar.tsx
│   │   │   ├── MobileNav.tsx
│   │   │   ├── AppHeader.tsx
│   │   │   └── SearchCommand.tsx
│   │   ├── entries/
│   │   │   ├── EntryCard.tsx
│   │   │   ├── EntryList.tsx
│   │   │   ├── EntryDetail.tsx
│   │   │   ├── EntryForm.tsx
│   │   │   └── EntryIcon.tsx
│   │   ├── groups/
│   │   │   ├── GroupTree.tsx
│   │   │   └── GroupForm.tsx
│   │   ├── generator/
│   │   │   ├── PasswordGenerator.tsx
│   │   │   ├── PassphraseGenerator.tsx
│   │   │   └── StrengthMeter.tsx
│   │   ├── import-export/
│   │   │   ├── ImportWizard.tsx
│   │   │   ├── ExportDialog.tsx
│   │   │   └── FormatSelector.tsx
│   │   ├── security/
│   │   │   ├── SecurityDashboard.tsx
│   │   │   ├── PasswordHealth.tsx
│   │   │   └── BreachCheck.tsx
│   │   ├── passkeys/
│   │   │   ├── PasskeyList.tsx
│   │   │   └── PasskeyDetail.tsx
│   │   └── shared/
│   │       ├── CopyButton.tsx
│   │       ├── RevealButton.tsx
│   │       ├── EmptyState.tsx
│   │       └── ErrorBoundary.tsx
│   ├── hooks/
│   │   ├── useTauriCommand.ts
│   │   ├── useDatabase.ts
│   │   ├── useEntries.ts
│   │   ├── usePasswordGenerator.ts
│   │   ├── useClipboard.ts
│   │   └── useTheme.ts
│   ├── stores/
│   │   ├── appStore.ts           # Zustand — UI state
│   │   ├── databaseStore.ts      # Database state
│   │   └── entriesStore.ts       # Entries cache
│   ├── lib/
│   │   ├── tauri.ts              # Tauri IPC wrapper
│   │   ├── crypto.ts             # Frontend crypto utils
│   │   ├── password-strength.ts  # zxcvbn wrapper
│   │   └── utils.ts
│   ├── types/
│   │   ├── entry.ts
│   │   ├── group.ts
│   │   ├── database.ts
│   │   └── import.ts
│   └── i18n/
│       ├── fr.json
│       └── en.json
│
├── src-tauri/                    # Backend Rust
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json
│   ├── src/
│   │   ├── main.rs               # Entry point
│   │   ├── lib.rs                # Tauri builder
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── database.rs       # KDBX open/save/lock
│   │   │   ├── entries.rs        # CRUD entries
│   │   │   ├── groups.rs         # CRUD groups
│   │   │   ├── generator.rs      # Password generation
│   │   │   ├── import.rs         # CSV/1PUX/Bitwarden
│   │   │   ├── export.rs         # CSV/JSON/HTML
│   │   │   ├── totp.rs           # TOTP operations
│   │   │   ├── passkeys.rs       # FIDO2/WebAuthn
│   │   │   ├── health.rs         # Password health check
│   │   │   └── browser.rs        # Browser integration bridge
│   │   ├── kdbx/
│   │   │   ├── mod.rs
│   │   │   ├── reader.rs         # KDBX parser
│   │   │   ├── writer.rs         # KDBX serializer
│   │   │   ├── crypto.rs         # AES/ChaCha20/Twofish
│   │   │   ├── keys.rs           # Key derivation (Argon2)
│   │   │   └── xml.rs            # KDBX XML layer
│   │   ├── import/
│   │   │   ├── mod.rs
│   │   │   ├── csv.rs
│   │   │   ├── onepassword.rs    # 1PUX + OPVault
│   │   │   ├── bitwarden.rs
│   │   │   ├── protonpass.rs
│   │   │   └── google_apple.rs   # CSV formats
│   │   ├── passkeys/
│   │   │   ├── mod.rs
│   │   │   ├── register.rs
│   │   │   └── authenticate.rs
│   │   └── security/
│   │       ├── mod.rs
│   │       ├── zxcvbn.rs         # Password strength
│   │       └── hibp.rs           # Have I Been Pwned (k-anonymity)
│   └── icons/                    # App icons
│
├── extension/                    # KeePassXC-Browser (modifiée)
│   ├── manifest.json             # Manifest V3
│   ├── background/
│   ├── content/
│   ├── popups/
│   └── css/                      # MyPass theme
│
├── public/
│   ├── manifest.json             # PWA manifest
│   ├── sw.js                     # Service Worker
│   └── icons/                    # PWA icons
│
├── package.json
├── vite.config.ts
├── tailwind.config.ts
├── tsconfig.json
└── components.json               # shadcn/ui config
```

---

## 7. Modèle de Données (KDBX)

### 7.1 Entrée (Entry)

```typescript
interface Entry {
  uuid: string;              // UUID v4
  group: string;             // Parent group UUID
  title: string;             // Nom affiché
  url: string;               // URL associée
  username: string;          // Nom d'utilisateur
  password: string;          // Mot de passe (protégé en mémoire)
  notes: string;             // Notes libres
  icon: number | string;     // Icône custom ou numéro
  tags: string[];            // Tags
  totp?: TotpConfig;         // Configuration TOTP
  passkey?: PasskeyData;     // Données Passkey
  customFields: Record<string, string>;  // Champs personnalisés
  attachments: Attachment[];
  expiry?: string;           // Date d'expiration ISO
  created: string;
  modified: string;
  history: EntryHistory[];
  autoType: AutoTypeConfig;
  browserSettings: BrowserSettings;
}
```

### 7.2 Groupe (Group)

```typescript
interface Group {
  uuid: string;
  name: string;
  parent?: string;           // Parent group UUID
  icon: number;
  children: string[];        // Child UUIDs
  entries: string[];         // Entry UUIDs
  isExpanded: boolean;       // UI state
}
```

### 7.3 Métadonnées Base

```typescript
interface DatabaseMeta {
  name: string;
  description: string;
  kdf: 'argon2id' | 'argon2d';
  encryption: 'aes256' | 'chacha20' | 'twofish';
  compression: 'gzip' | 'none';
  version: '4.0' | '4.1';
  created: string;
  modified: string;
}
```

---

## 8. Commandes Tauri (IPC Bridge)

Toutes les opérations passent par des commandes Tauri. Le frontend ne manipule jamais directement le fichier KDBX.

```rust
// === Database ===
#[tauri::command] async fn open_database(path: String, password: String, keyfile: Option<String>) -> Result<DatabaseInfo>
#[tauri::command] async fn create_database(path: String, password: String, meta: DatabaseMeta) -> Result<DatabaseInfo>
#[tauri::command] async fn save_database() -> Result<()>
#[tauri::command] async fn lock_database() -> Result<()>
#[tauri::command] async fn change_master_password(old: String, new: String) -> Result<()>

// === Entries ===
#[tauri::command] async fn get_entries(group_uuid: Option<String>) -> Result<Vec<Entry>>
#[tauri::command] async fn get_entry(uuid: String) -> Result<Entry>
#[tauri::command] async fn create_entry(entry: NewEntry) -> Result<Entry>
#[tauri::command] async fn update_entry(uuid: String, entry: UpdateEntry) -> Result<Entry>
#[tauri::command] async fn delete_entry(uuid: String) -> Result<()>
#[tauri::command] async fn search_entries(query: String) -> Result<Vec<Entry>>
#[tauri::command] async fn duplicate_entry(uuid: String) -> Result<Entry>

// === Groups ===
#[tauri::command] async fn get_groups() -> Result<Vec<Group>>
#[tauri::command] async fn create_group(name: String, parent: Option<String>) -> Result<Group>
#[tauri::command] async fn update_group(uuid: String, group: UpdateGroup) -> Result<Group>
#[tauri::command] async fn delete_group(uuid: String) -> Result<()>
#[tauri::command] async fn move_entry(entry_uuid: String, group_uuid: String) -> Result<()>

// === Generator ===
#[tauri::command] async fn generate_password(config: PasswordConfig) -> Result<String>
#[tauri::command] async fn generate_passphrase(config: PassphraseConfig) -> Result<String>
#[tauri::command] async fn evaluate_strength(password: String) -> Result<StrengthResult>

// === TOTP ===
#[tauri::command] async fn get_totp(entry_uuid: String) -> Result<String>

// === Import/Export ===
#[tauri::command] async fn import_csv(path: String, mapping: ColumnMapping) -> Result<ImportResult>
#[tauri::command] async fn import_1password(path: String, password: Option<String>) -> Result<ImportResult>
#[tauri::command] async fn import_bitwarden(path: String, password: Option<String>) -> Result<ImportResult>
#[tauri::command] async fn import_google_passwords(path: String) -> Result<ImportResult>
#[tauri::command] async fn import_apple_passwords(path: String) -> Result<ImportResult>
#[tauri::command] async fn export_csv(path: String, entries: Vec<String>) -> Result<()>
#[tauri::command] async fn export_json(path: String, entries: Vec<String>) -> Result<()>

// === Security ===
#[tauri::command] async fn check_password_health() -> Result<HealthReport>
#[tauri::command] async fn check_hibp(prefix: String) -> Result<Vec<BreachInfo>>

// === Passkeys ===
#[tauri::command] async fn register_passkey(entry_uuid: String, challenge: String) -> Result<PasskeyData>
#[tauri::command] async fn authenticate_passkey(entry_uuid: String, credential: String) -> Result<bool>
#[tauri::command] async fn export_passkeys(uuids: Vec<String>, path: String) -> Result<()>
#[tauri::command] async fn import_passkeys(path: String, group_uuid: String) -> Result<ImportResult>

// === Browser ===
#[tauri::command] async fn start_native_messaging() -> Result<()>
#[tauri::command] async fn stop_native_messaging() -> Result<()>
#[tauri::command] async fn get_browser_status() -> Result<BrowserStatus>
```

---

## 9. Flux Utilisateurs Principaux

### 9.1 Création de Base de Données

```
1. User clique "Nouveau coffre"
2. Formulaire : nom, master password (x2), hint, keyfile optionnel
3. Choix chiffrement (AES/ChaCha20), KDF (Argon2id)
4. Backend crée fichier .kdbx vide chiffré
5. Navigation vers la vue principale
```

### 9.2 Ouverture de Base

```
1. User clique "Ouvrir coffre" ou drag-drop fichier .kdbx
2. Dialog : master password + keyfile optionnel
3. Backend déchiffre, vérifie intégrité
4. Frontend charge entries/groups via IPC
5. Navigation vers Dashboard
```

### 9.3 Création d'Entrée

```
1. Bouton "+" ou Ctrl+N
2. Sheet/Dialog avec formulaire complet
3. Générateur de mot de passe intégré
4. Sauvegarde → backend écrit dans KDBX en mémoire
5. Notification toast "Entrée créée"
```

### 9.4 Import

```
1. Menu "Importer" → choix du format
2. Sélection fichier + options format
3. Preview des données à importer
4. Choix groupe destination ou nouvelle base
5. Import → feedback compteur
```

### 9.5 Auto-fill Navigateur

```
1. Base ouverte dans MyPass
2. Extension détecte connexion
3. Sur un site, icône KeePassXC dans les champs
4. Clic → liste des credentials correspondants
5. Sélection → remplissage automatique
```

---

## 10. Sécurité

### 10.1 Principes

- **Zero-knowledge** : Le master password n'est jamais stocké
- **Memory protection** : Les mots de passe sont `zeroize`-d après usage en Rust
- **Clipboard** : Effacé automatiquement après 30s (configurable)
- **Lock automatique** : Après N minutes d'inactivité
- **Anti-screenshot** : Option pour bloquer les captures d'écran (via Tauri)
- **HIBP k-anonymity** : Vérification par préfixe SHA-1 uniquement

### 10.2 Chiffrement en Transit

- IPC Tauri : local uniquement, pas de réseau
- Extension → Tauri : NaCl box (TweetNaCl.js) chiffré
- Pas de données en clair sur le disque

---

## 11. Plan Pas-à-Pas (MVP)

### Phase 0 — Setup Projet

- [ ] **0.1** Initialiser projet Tauri v2 + React + Vite + TypeScript
- [ ] **0.2** Installer Tailwind CSS v4 + shadcn/ui (theme nova)
- [ ] **0.3** Configurer les composants shadcn essentiels
- [ ] **0.4** Mettre en place le design system (tokens CSS, polices)
- [ ] **0.5** Configurer ESLint + Prettier + Husky
- [ ] **0.6** Setup PWA (vite-plugin-pwa, manifest, service worker)

### Phase 1 — Backend Rust Core

- [ ] **1.1** Implémenter module KDBX reader (parser XML + déchiffrement AES/ChaCha20)
- [ ] **1.2** Implémenter dérivation de clés (Argon2id)
- [ ] **1.3** Implémenter module KDBX writer (sérialisation + chiffrement)
- [ ] **1.4** Créer les commandes Tauri `open_database`, `create_database`, `save_database`, `lock_database`
- [ ] **1.5** Implémenter générateur de mots de passe (CSPRNG)
- [ ] **1.6** Implémenter évaluateur de force (zxcvbn port ou bindings)
- [ ] **1.7** Implémenter générateur de passphrase
- [ ] **1.8** Tests unitaires pour les modules crypto

### Phase 2 — CRUD Entrées & Groupes

- [ ] **2.1** Commandes Tauri CRUD entries (create, read, update, delete, search)
- [ ] **2.2** Commandes Tauri CRUD groups (create, read, update, delete, move)
- [ ] **2.3** Commandes Tauri TOTP (generate, get code)
- [ ] **2.4** Tests d'intégration CRUD

### Phase 3 — Frontend Core UI

- [ ] **3.1** Layout principal : Sidebar + Main (responsive desktop/mobile)
- [ ] **3.2** Écran de déverrouillage (UnlockView)
- [ ] **3.3** Dashboard / liste des entrées (EntryList + EntryCard)
- [ ] **3.4** Vue détail d'une entrée (EntryDetail)
- [ ] **3.5** Formulaire création/édition (EntryForm)
- [ ] **3.6** Arbre des groupes (GroupTree) + sidebar
- [ ] **3.7** Barre de recherche avec Command palette (⌘K)
- [ ] **3.8** Navigation mobile (BottomTabBar)
- [ ] **3.9** Stores Zustand (app, database, entries)
- [ ] **3.10** Hooks useTauriCommand, useDatabase, useEntries

### Phase 4 — Générateur & Sécurité

- [ ] **4.1** UI Générateur de mot de passe (PasswordGenerator avec sliders)
- [ ] **4.2** UI Générateur de passphrase
- [ ] **4.3** Indicateur de force visuel (StrengthMeter avec barre + feedback)
- [ ] **4.4** Dashboard sécurité (SecurityDashboard — mots de passe faibles/réutilisés/anciens)
- [ ] **4.5** Intégration HIBP (Have I Been Pwned) via k-anonymity
- [ ] **4.6** Auto-lock timer + options de sécurité

### Phase 5 — Import / Export

- [ ] **5.1** Module Rust import CSV (mapping colonnes)
- [ ] **5.2** Module Rust import 1Password (1PUX + OPVault)
- [ ] **5.3** Module Rust import Bitwarden (JSON)
- [ ] **5.4** Module Rust import Google Passwords (CSV)
- [ ] **5.5** Module Rust import Apple Passwords (CSV)
- [ ] **5.6** Module Rust import Proton Pass (JSON)
- [ ] **5.7** Module Rust export CSV / JSON
- [ ] **5.8** UI Import Wizard (étapes : format → fichier → preview → import)
- [ ] **5.9** UI Export Dialog (format, sélection d'entrées)

### Phase 6 — Passkeys

- [ ] **6.1** Module Rust Passkeys (FIDO2/WebAuthn register/authenticate)
- [ ] **6.2** Commandes Tauri passkeys
- [ ] **6.3** UI Gestion des passkeys (liste, import/export)
- [ ] **6.4** Intégration avec le flux navigateur

### Phase 7 — Intégration Navigateur

- [ ] **7.1** Adapter KeePassXC-Browser pour MyPass (rebranding, thème)
- [ ] **7.2** Implémenter Native Messaging bridge côté Tauri
- [ ] **7.3** Tester le flux complet auto-fill
- [ ] **7.4** Tester le flux passkeys navigateur
- [ ] **7.5** Support WebSocket fallback

### Phase 8 — Polish & PWA

- [ ] **8.1** Mode sombre/clair (toggle + détection système)
- [ ] **8.2** Animations et transitions (Framer Motion ou CSS)
- [ ] **8.3** États vides, loading, erreur pour chaque vue
- [ ] **8.4** Responsive complet (mobile-first)
- [ ] **8.5** PWA : installation, offline, notifications
- [ ] **8.6** i18n FR + EN
- [ ] **8.7** Raccourcis clavier
- [ ] **8.8** Tests E2E (Playwright)

---

## 12. Non-Goals (Explicitement Exclus du MVP)

- ❌ Synchronisation cloud (KeeShare local seulement)
- ❌ Apps mobiles natives (React Native) — PWA seulement
- ❌ SSH Agent integration
- ❌ Secret Service integration (Linux)
- ❌ Auto-Type (application desktop externe)
- ❌ Support KeePass 1 (.kdb)
- ❌ Remote database import
- ❌ Custom icons download

---

## 13. Risques Identifiés

| Risque | Impact | Mitigation |
|---|---|---|
| Implémentation KDBX4 complexe | Élevé | Forker `kdbx-rs` existant, utiliser KeePassXC comme référence |
| Compatibilité KeePassXC-Browser | Moyen | Protocole documenté, extension éprouvée |
| Passkeys cross-plateforme | Élevé | Limiter à desktop pour MVP, tester Chrome/Firefox |
| Performance gros fichiers KDBX | Moyen | Pagination, lazy loading des entrées |
| Sécurité mémoire (Rust) | Faible | Rust memory safety + zeroize |

---

## 14. Assomptions

1. Le protocole NaCl de KeePassXC-Browser est suffisamment documenté pour être reproduit côté Rust
2. La crate `kdbx-rs` ou une implémentation Rust de KDBX existe ou peut être forké
3. L'utilisateur a Node.js 20+, Rust 1.80+, et les outils Tauri installés
4. Le format Google Passwords CSV suit une structure connue et stable
5. Le format Apple Passwords CSV suit une structure connue et stable

---

## 15. Checklist de Validation

- [ ] Le plan couvre-t-il toutes les features demandées ?
- [ ] L'architecture Tauri est-elle correctement dimensionnée ?
- [ ] Les risques sont-ils acceptables ?
- [ ] Le périmètre MVP est-il clairement défini ?
- [ ] Les non-goals sont-ils explicites ?
- [ ] Le design system est-il approuvé ?
