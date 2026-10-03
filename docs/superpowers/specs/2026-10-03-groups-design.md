# Groupes — Design (sous-projet 4)

> Sous-projet 4 de `docs/superpowers/specs/2026-10-02-roadmap-design.md`.
> Design validé avec Luca le 2026-10-03.

## Intention

Luca range ses entrées en dossiers, sur ses deux PC et son téléphone. Un dossier créé, renommé ou
supprimé, et une entrée déplacée, se retrouvent à l'identique partout après la sync.

**Succès :** un dossier créé ou renommé sur un appareil apparaît sur l'autre après sync ; supprimer
un dossier fait remonter son contenu au parent sur tous les appareils, sans que le dossier
réapparaisse ; une entrée déplacée change de dossier partout.

## Constat de départ

- Commandes Tauri (`src-tauri/src/commands/groups.rs`) et bindings wasm (`get_groups`,
  `create_group`, `update_group`, `delete_group`, `move_entry`) existent, testés.
- UI non branchée : `AppSidebar.tsx` ne passe jamais `groups` à `GroupTree` et `onSelectGroup`
  ne fait rien ; « Renommer » / « Supprimer » sans action ; aucun cas groupe dans `src/lib/web.ts` ;
  le mock renvoie une racine vide.
- Types TS faux : `src/types/group.ts` attend `isExpanded`, `entries: string[]`, `icon: number`
  alors que `GroupInfo` sérialise `is_expanded`, `entry_count`, `parent_uuid` (toujours `None`) ;
  `Entry.group` (TS) n'existe pas côté Rust, qui envoie `group_uuid`.
- `groups::update` n'avance pas le `LastModificationTime` du groupe.
- `groups::delete` supprime le dossier **et** ses entrées (une tombstone par entrée).
- `update_entry` contient une branche `group_uuid` morte (« skip for now »).
- Fusion (`crates/mypass-core/src/merge.rs`) : elle n'ajoute que les groupes inconnus. Donc
  - un renommage ne se propage pas (et l'ancien nom peut revenir) ;
  - une entrée déplacée garde son ancien dossier chez l'autre (le gagnant LWW remplace l'entrée
    sur place) ;
  - un dossier supprimé n'est pas supprimé ailleurs, et l'autre appareil le recrée vide au merge
    suivant.
- Le push desktop dépend de `merge(remote_view, local).changed()` (`commands/sync.rs`) : un
  changement de dossier qui n'y compte pas n'est jamais poussé.
- Sync et import n'invalident que la requête `["entries"]`, alors que l'import crée des dossiers.

## Décisions

| Sujet | Décision |
|---|---|
| Suppression d'un dossier | Entrées et sous-dossiers **remontent au parent** ; seul le dossier disparaît. |
| Fusion des dossiers | Approche A : LWW sur le dossier (nom, icône, notes), tombstones de dossiers, l'entrée gagnante porte son dossier. |
| Déplacer une entrée | Par menu dans le détail ; pas de glisser-déposer. |
| Déplacer un dossier | Hors périmètre (ni UI ni fusion). |
| Filtrage | Côté client, sur la liste déjà chargée ; dossier sélectionné = ses entrées + celles des sous-dossiers. |
| Filtre dossier / filtre type | Exclusifs : choisir l'un efface l'autre. |
| État replié/déplié | Non persisté (état initial = `IsExpanded` du coffre). |

## 1. Opérations (`crates/mypass-core/src/ops/groups.rs`)

- `create(kf, name, parent)` et `update(kf, uuid, name, icon, is_expanded)` : nom trimé ; vide →
  `Err("GROUP_NAME_REQUIRED")`. `update` qui change le nom ou l'icône appelle `times.touch()`
  (pas pour `is_expanded` seul).
- `delete(kf, uuid)` : racine refusée (inchangé). Entrées et sous-dossiers du dossier sont ajoutés
  à la fin du parent ; chaque entrée et chaque sous-dossier remontés ont leur `LocationChanged`
  avancé, **pas** le `LastModificationTime` des entrées (une modification faite entre-temps sur un
  autre appareil doit continuer à gagner ; cet appareil dissout le dossier dans le même parent).
  Le dossier est retiré ; **une seule** tombstone, celle du dossier. *(Amendé après la revue
  finale : la première version avançait aussi le LMT et masquait une modification plus récente.)*
- `move_entry` : avance aussi `LocationChanged` (compat KeePassXC) en plus du `touch()` existant.
- `GroupInfo` : `#[serde(rename_all = "camelCase")]`, champ `parent_uuid` supprimé,
  `entry_count` = entrées du dossier **et de ses sous-dossiers**.
- `ops/entries.rs` : branche morte `group_uuid` de `update` et champ `UpdateEntry.group_uuid`
  supprimés (serde ignore les champs inconnus : un client qui l'envoie encore n'échoue pas).

## 2. Fusion (`crates/mypass-core/src/merge.rs`)

Ordre dans `merge(local, remote)` :

1. **Groupes distants** (parcours de l'arbre distant, racine exclue) :
   - inconnu en local : s'il existe une tombstone locale `t >= LMT(groupe distant)`, il n'est pas
     recréé et ses enfants sont rattachés au parent effectif du groupe ignoré (parent le plus
     proche qui existe) ; sinon il est ajouté vide sous son parent (racine si introuvable),
     `groups_added += 1` (comportement existant) ;
   - connu en local avec `LMT(distant) > LMT(local)` : `name`, `icon_id`, `notes` et `times`
     du distant sont copiés, `groups_updated += 1`. Les enfants et entrées ne sont pas touchés.
2. **Entrées** (existant) : quand le distant gagne, en plus du remplacement, l'entrée est déplacée
   dans le dossier parent distant si celui-ci existe en local et diffère de l'actuel. Un parent
   distant supprimé ici (non recréé en passe 1) est remplacé par le parent survivant qui le
   remplace, pour les entrées mises à jour comme pour les nouvelles : l'autre appareil les range
   au même endroit en dissolvant le dossier.
3. **Suppressions d'entrées** (existant).
4. **Suppressions de dossiers** : tout groupe local (racine exclue) absent du distant et ayant une
   tombstone distante `t >= LMT(groupe local)` est retiré ; son contenu restant (entrées et
   sous-dossiers) est ajouté à son parent, puis `groups_deleted += 1`. Rien n'est perdu, même
   une entrée ajoutée ailleurs entre-temps.
5. **Union des tombstones** (existant).

- `MergeOutcome` gagne `groups_updated` et `groups_deleted` ; `changed()` les compte.
- Le merge reste idempotent : rejouer la même fusion ne change rien et renvoie `changed() == false`.
- **Limite assumée** (commentaire `ponytail:` dans `merge.rs`) : la position d'un dossier n'est
  pas fusionnée. Seul cas visible : un même dossier supprimé sur un appareil et renommé sur un
  autre avant sync — le dossier renommé survit, mais ses sous-dossiers et ses entrées peuvent
  rester à des endroits différents selon l'appareil.
- **Limite assumée de l'approche A** : déplacer une entrée sur un appareil puis la modifier sur un
  autre avant sync → la modification (plus récente) gagne et ramène l'entrée dans son ancien
  dossier. Rien n'est perdu.

## 3. Parité desktop / PWA / mock

- Desktop : les 5 commandes Tauri existent, enregistrées dans `generate_handler!` ; inchangées.
- `src/lib/web.ts` : cas `get_groups`, `create_group`, `update_group`, `delete_group`,
  `move_entry` sur les bindings wasm existants ; chaque mutation appelle `schedulePush()`.
  Formes JSON identiques à l'IPC.
- `src/lib/mock.ts` : arbre de dossiers en mémoire (racine + au moins un dossier), CRUD et
  `move_entry` ; les entrées mock portent `group_uuid`.

## 4. Interface

- **Types** : `src/types/group.ts` reflète `GroupInfo` (`uuid`, `name`, `icon`, `children`,
  `entryCount`, `isExpanded`, `created`, `modified`) ; `Entry.group` devient `group_uuid`.
- **`useGroups()`** (`src/hooks/useGroups.ts`) : requête `["groups"]` + mutations
  `createGroup`, `renameGroup`, `deleteGroup`, `moveEntry` ; chaque mutation invalide `["groups"]`
  et `["entries"]`. `useSync` et `ImportWizard` invalident aussi `["groups"]`.
- **Store** (`entriesStore`) : `groupFilter: string | null` + `setGroupFilter`. `setGroupFilter`
  remet `kindFilter` à `null`, `setKindFilter` remet `groupFilter` à `null` ; les deux vident la
  sélection d'entrée.
- **Barre latérale** (`AppSidebar` desktop et tiroir mobile) : section « Dossiers » avec bouton
  « + », arbre des enfants de la racine (racine cachée) ; le bouton « Tous les éléments » interne
  à `GroupTree` est retiré ; le « Tous les éléments » de la navigation efface les deux filtres.
  Compteur = `entryCount`. Choisir un dossier navigue vers `/` et ferme le tiroir mobile
  (`onNavigate`). Menu « ⋯ » : Ajouter un sous-dossier, Renommer, Supprimer.
- **Liste** (`EntryList`) : si `groupFilter`, ne garde que les entrées dont `group_uuid` est le
  dossier choisi ou un de ses descendants (calculé depuis l'arbre). La recherche ⌘K reste globale.
- **Dialogues** :
  - `GroupNameDialog` unique pour créer / renommer (champ prérempli au renommage, Entrée valide,
    nom vide refusé côté formulaire, `GROUP_NAME_REQUIRED` traduit) ;
  - suppression : `AlertDialog` « Supprimer le dossier X ? Ses entrées et sous-dossiers
    remonteront dans le dossier parent. » ; si le dossier supprimé était filtré → `groupFilter`
    à `null`.
- **Création d'entrée** : `EntryForm`, `ItemForm`, `SshKeyForm` passent `groupUuid: groupFilter`
  à la création quand un dossier est sélectionné.
- **Détail** (`EntryDetail`) : bouton « Déplacer » (icône dossier) dans l'en-tête → menu des
  dossiers en arbre (indentés), « Aucun dossier » (racine) en tête, dossier actuel coché ; choisir
  appelle `move_entry`. Sous le titre, le nom du dossier de l'entrée s'il n'est pas à la racine.
- **i18n** : toutes les nouvelles chaînes en EN et FR (`groups.*`).

## 5. Vérification

- Rust (`mypass-core`) :
  - nom vide refusé à la création et au renommage ; renommer avance le LMT ;
  - `delete` : contenu remonté au parent, une seule tombstone (le dossier), entrées remontées
    touchées ;
  - fusion : renommage le plus récent gagne dans les deux sens ; dossier supprimé non recréé et
    retiré de l'autre côté avec contenu remonté ; entrée ajoutée ailleurs dans un dossier
    supprimé survit au parent ; entrée déplacée suit son dossier ; idempotence (`changed()`
    faux au second passage) ; `changed()` vrai pour un renommage seul et une suppression seule.
- `npm run smoke:wasm`, tests `src-tauri`, serveur, wasm ; `tsc`, lint, builds.
- E2E PWA locale (serveur local, coffre jetable) : créer, renommer, filtrer, déplacer, supprimer.
- E2E desktop (`tauri dev`, `APPDATA` temporaire) : parcours de base.
- Sync à trois (serveur local, deux coffres jetables) : renommage, suppression et déplacement
  faits d'un côté visibles de l'autre.
- Jamais contre la prod ni les vrais coffres.

## Hors périmètre

Glisser-déposer, déplacer un dossier, corbeille (un groupe `RecycleBinUUID` venu de KeePassXC
s'affiche comme un dossier ordinaire), persistance de l'état replié/déplié, icônes de dossier
personnalisées.
