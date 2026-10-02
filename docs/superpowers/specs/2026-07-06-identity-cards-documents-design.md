# Identités, cartes bancaires et documents — Design

**Date** : 2026-07-06
**Statut** : validé par l'utilisateur (périmètre v1 : stockage + autofill navigateur, approche A)

## Objectif

Reproduire la fonctionnalité 1Password « Identités / Cartes / Documents » dans MyPass :
enregistrer une identité, des cartes bancaires et des papiers officiels dans le coffre,
puis remplir les formulaires web (inscription, adresse, paiement) en un clic depuis le
navigateur via l'extension keepassxc-browser forkée.

## Décisions actées

- **Périmètre v1** : les trois types (identité, carte, document) + autofill navigateur pour identité et carte. Les documents ne sont pas exposés au navigateur.
- **Autofill** : approche A — extension du protocole Native Messaging (`get-identities`) + modification du fork keepassxc-browser (détection de champs + menu de remplissage). Approches rejetées : stringFields `KPH:` (configuration manuelle par site, UX médiocre), Auto-Type desktop (fragile, pas navigateur-natif).
- **Stockage** : entrées KDBX standard avec champs personnalisés préfixés — aucun nouveau format, fichier toujours ouvrable dans KeePassXC, chiffrement inchangé.

## 1. Modèle de données

Chaque élément est une entrée KDBX avec un champ personnalisé discriminant `MyPass_Type` :
`identity` | `card` | `document`. Les entrées classiques (logins) n'ont pas ce champ.

Champs par type (customFields KDBX ; « protégé » = flag Protected dans le XML, comme les mots de passe) :

| Type | Champs |
|---|---|
| Identité | `ID_FirstName`, `ID_LastName`, `ID_BirthDate`, `ID_Email`, `ID_Phone`, `ID_Address`, `ID_City`, `ID_PostalCode`, `ID_Country`, `ID_Company` |
| Carte | `CC_Holder`, `CC_Number` (protégé), `CC_ExpMonth`, `CC_ExpYear`, `CC_CVC` (protégé), `CC_Type` (visa/mastercard/amex/autre, déduit du numéro) |
| Document | `DOC_Kind` (passport/id_card/driver_license/social_security/other), `DOC_Number` (protégé), `DOC_IssueDate`, `DOC_ExpiryDate`, `DOC_Country` |

- Le `title` de l'entrée sert de nom d'affichage (« Visa perso », « Passeport Luca »).
- Scans de documents : **reportés en v1.1** — le pool binaire KDBX4 (contenu des pièces jointes) n'est pas implémenté dans `reader.rs`/`writer.rs`, contrairement à ce qui était supposé initialement. Les documents v1 stockent numéros, dates et notes.
- Dates au format ISO `YYYY-MM-DD` dans les champs.
- Côté TS : type `ItemKind = "login" | "identity" | "card" | "document"` dérivé de `MyPass_Type`, helpers de lecture/écriture des customFields typés (pas de nouveau type d'entrée dans le store — l'`Entry` existant suffit).

## 2. UI de l'app (React)

- **Barre latérale** : sections « Identités », « Cartes », « Documents » à côté des groupes — filtres sur `MyPass_Type` (pas des groupes KDBX ; un élément vit dans n'importe quel groupe).
- **Création** : le bouton « Nouvelle entrée » propose le type ; `EntryForm` rend une variante par type avec les champs dédiés (au lieu de username/password/URL). Carte : détection du réseau (Visa/MC/Amex) depuis le numéro, saisie expiration MM/AA.
- **Détail** (`EntryDetail`) : variante par type ; chaque champ a un bouton copier ; numéro de carte et CVC masqués par défaut, révélables ; rendu visuel type « carte bancaire » pour les cartes.
- **Liste** (`EntryList`/`EntryCard`) : icône et sous-titre adaptés au type (ex. « Visa •••• 4242 »).
- i18n FR/EN via react-i18next pour toutes les chaînes.
- Recherche existante : fonctionne telle quelle (titre + champs).

## 3. Protocole Native Messaging (Rust)

Nouvelle action `get-identities` dans `src-tauri/src/native_messaging.rs`, enregistrée dans
le `match` des actions, même chiffrement NaCl et même contrôle d'association que `get-logins` :

- Requête : `{ action: "get-identities" }` (pas de filtre URL — les identités/cartes ne sont pas liées à un site).
- Réponse : `{ identities: [{ uuid, title, type: "identity"|"card", fields: {…} }] }` — uniquement les entrées `MyPass_Type` ∈ {identity, card}, avec leurs champs `ID_*`/`CC_*` déchiffrés. Les documents sont exclus.
- Coffre verrouillé → même comportement/erreur que `get-logins`.
- Tests unitaires Rust sur le handler (coffre avec entrées mixtes → seuls identity/card renvoyés, documents et logins exclus).

## 4. Extension navigateur (fork `keepassxc-browser/keepassxc-browser/`)

Sur le modèle de `totp-field.js` / `totp-autocomplete.js` :

- **`content/identity-fields.js`** (nouveau) : détection des champs remplissables d'un formulaire :
  1. attribut `autocomplete` standard (`cc-number`, `cc-exp`, `cc-csc`, `cc-name`, `given-name`, `family-name`, `email`, `tel`, `street-address`, `address-line1/2`, `postal-code`, `country`, `bday`…) ;
  2. à défaut, heuristiques regex FR+EN sur `name`/`id`/`placeholder`/`<label>` (ex. `prenom|first.?name`, `carte|card.?number`, `cvc|cvv|crypto`).
  - Un formulaire est « paiement » s'il contient un champ cc-number, « identité/adresse » sinon (s'il a ≥ 2 champs identité).
- **UI dans la page** : icône MyPass dans le premier champ détecté (réutilise `icon.js`/`ui.js`) ; clic → menu autocomplete listant les cartes (formulaire paiement) ou identités (formulaire identité) par titre ; sélection → remplissage de tous les champs détectés du formulaire (valeur + événements input/change comme `fill.js` le fait déjà).
- **`background/`** : nouveau message `get_identities` relayé au host via l'action protocole `get-identities`. Les données ne sont demandées qu'au clic sur l'icône — rien n'est pré-chargé dans la page.
- Mapping champ détecté → champ MyPass : table statique (ex. `cc-number` → `CC_Number`, `given-name` → `ID_FirstName`, `cc-exp` → `MM/YY` composé depuis `CC_ExpMonth`+`CC_ExpYear`).

## 5. Sécurité

- CVC et numéros stockés protégés dans le KDBX (chiffrement mémoire interne KeePass).
- Transport navigateur : NaCl box existant, association existante — aucune nouvelle surface d'authentification.
- Les documents ne transitent jamais vers le navigateur.
- Pas de remplissage automatique sans geste utilisateur (clic sur l'icône puis choix dans le menu).

## 6. Phases de livraison

1. **App** : helpers de types TS + variantes EntryForm/EntryDetail/EntryList + sections latérales + i18n. Utilisable seule (copie un clic).
2. **Rust** : action `get-identities` + tests unitaires.
3. **Extension** : détection de champs, menu, remplissage ; vérification manuelle de bout en bout (`npm run tauri dev` + extension non-empaquetée) sur des formulaires réels (checkout de test, formulaire d'inscription).

## Hors périmètre v1

- Scans de documents en pièce jointe (nécessite d'implémenter le pool binaire KDBX4 dans reader/writer — v1.1).
- Autofill des documents (copie un clic seulement).
- Plusieurs adresses par identité, adresses de livraison/facturation distinctes.
- Synchronisation du fork extension avec l'upstream keepassxc-browser.
- Soumission automatique des formulaires après remplissage.
