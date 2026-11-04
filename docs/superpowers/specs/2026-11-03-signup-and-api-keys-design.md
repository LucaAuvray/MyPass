# Inscription assistée et clés API — Design

> Design validé avec Luca le 2026-11-03 (brainstorming). Trois morceaux, livrés dans cet ordre :
> 1. suggestion de mot de passe à l'inscription + enregistrement fiable ;
> 2. type d'élément « Clé API » dans l'app ;
> 3. détection des clés API dans l'extension.

## Intention

Quand Luca crée un compte sur un site, MyPass lui **propose un mot de passe fort** dans le champ,
puis **propose d'enregistrer le compte** — et cette proposition doit apparaître à chaque fois, pas
« sur certains sites ». Quand il crée une **clé API** (exemple réel : la console Mistral, qui
n'affiche la clé qu'une fois), MyPass la **repère seul** et propose de la ranger dans le coffre.

Règle commune, choisie par Luca : **rien ne s'enregistre sans son clic, et un seul clic suffit.**

**Succès :**
- sur un formulaire d'inscription, cliquer dans le champ mot de passe ouvre la suggestion ; un clic
  remplit le mot de passe et sa confirmation ;
- après l'inscription, la bannière « Enregistrer » apparaît, y compris quand l'inscription se fait
  en JavaScript, en plusieurs étapes, ou atterrit sur une page sans champ ; un clic crée l'entrée, qui
  apparaît dans l'app sans attendre ;
- à la création d'une clé sur console.mistral.ai, une bannière « Clé API détectée » apparaît ; un
  clic crée un élément « Clé API » ; revenir sur la page ne la repropose pas.

## Constat de départ

Extension : fork de KeePassXC-Browser 1.10.3 (`keepassxc-browser/keepassxc-browser/`, MV3, chargée
non empaquetée). Hôte : `src-tauri/src/native_messaging.rs` + `commands/browser.rs`.

**Pas de suggestion à l'inscription**
- L'icône du générateur est désactivée par défaut (`usePasswordGeneratorIcons: false`,
  `background/page.js:46`) et, même active, n'est qu'une icône à cliquer (`content/pwgen.js`),
  posée aussi sur les formulaires de connexion (`content/icon-handler.js:67`).
- Rien n'exploite `autocomplete="new-password"` pour proposer quoi que ce soit
  (`content/fields.js:390` ne s'en sert que pour ne pas remplir).
- Le générateur de l'hôte est ad hoc (20 caractères, son propre jeu, `native_messaging.rs:729`), pas
  celui de `mypass-core` (`generator.rs`, `PasswordConfig`).
- Le clic droit → « Générateur de mot de passe » existe déjà (`background/init.js:8`).

**Enregistrement qui rate**
- L'envoi n'est vu que par l'événement `submit` d'un `<form>` ou le clic sur *le* bouton que
  `kpxcForm.getFormSubmitButton` a choisi (`content/form.js:64-113, 163-181`). Les inscriptions en
  JavaScript (bouton `<div>`/`<a>`, `fetch`, Entrée dans le champ) passent inaperçues.
- Les identifiants envoyés sont gardés dans la **mémoire du service worker**
  (`page.submittedCredentials`, `background/page.js:67, 178`). En MV3, Chrome l'arrête après ~30 s
  d'inactivité : une étape « vérifiez votre e-mail » suffit à les perdre.
- Sur la page suivante, `initContentScript` **abandonne si l'identifiant est vide** et attend
  `retrieveCredentialsCallback` (`content/keepassxc-browser.js:876-879`), qui ne tourne que sur une
  page ayant des champs de connexion (`initCredentialFields`, l. 358-393). Un tableau de bord après
  inscription n'en a pas : la bannière n'apparaît jamais.
- Au-delà de `redirectAllowance` (3) redirections, les identifiants envoyés sont effacés
  (l. 881-883) : fréquent dans les parcours d'inscription.
- L'hôte crée l'entrée à la racine avec **l'URL complète comme titre**
  (`commands/browser.rs:240`, `Entry::new(url, …)`).
- L'app ne relit la liste qu'au tick de sync de 60 s (`src/components/layout/AppShell.tsx:24`) :
  une entrée enregistrée depuis le navigateur semble absente pendant une minute.

**Clés API** : aucun type ; les types existants (`identity`, `card`, `document`, `ssh_key`) sont
des entrées KDBX discriminées par `MyPass_Type` (`src/lib/items.ts`), champs secrets protégés par
`PROTECTED_CUSTOM_KEYS` (`crates/mypass-core/src/ops/entries.rs:347`).

## Objectifs / non-objectifs / anti-objectifs

**Objectifs** : ceux du « Succès » ci-dessus, sur Chrome et Edge (extension actuelle).

**Non-objectifs**
- Enregistrement silencieux sans clic (écarté par Luca).
- Remplir une clé API dans une page : on la copie depuis l'app.
- Rappels d'expiration ou de rotation des clés ; paire identifiant + secret (AWS…) au-delà des
  notes.
- Détecter d'autres secrets affichés (codes de récupération, clés SSH) ; Firefox.
- Réglages utilisateur pour tout ça : comportement fixe, pas de nouvelles options.

**Anti-objectifs** (échec même si « ça marche »)
- Une bannière qui revient sans cesse : sur chaque page affichant un hash de commit, ou à chaque
  visite d'une page qui montre en permanence une clé déjà rangée.
- Une suggestion de mot de passe qui s'ouvre sur un formulaire de **connexion**.
- Un contenu de page (clé, mot de passe) envoyé ailleurs qu'à l'app MyPass locale, ou loggé.
- Casser le remplissage automatique existant ou le protocole KeePassXC.

## Approches

| | Approche | Verdict |
|---|---|---|
| **A** | **Étendre le fork existant** : suggestion dans `pwgen.js`, mot de passe généré mémorisé par onglet, nouveau script de détection des clés, nouvelles actions d'hôte, type `api_key` dans core + app. | **Retenue** (choix de Luca) : réutilise la détection de formulaires rodée, la bannière, le canal NaCl. |
| B | Nouvelle extension MyPass from scratch | Écartée : on perd toute la détection de KeePassXC-Browser, des semaines de travail. |
| C | Intercepter les requêtes réseau de la page (`fetch`/XHR) pour voir passer le mot de passe | Écartée : fragile selon les sites, intrusif, inutile si on mémorise le mot de passe généré. |
| D | Réutiliser `set-login` pour les clés | Écartée : la clé arriverait en identifiant, sans type ni protection dédiée. |

Dans A, Luca a retenu l'**interception des boutons « Copier »** faits en JavaScript
(`navigator.clipboard.writeText`) par un script injecté dans la page, comme pour les passkeys
(`content/passkeys-inject.js`), plutôt que le seul Ctrl+C.

## Morceau 1 — Inscription : suggestion et enregistrement

### Ce qui est décidé (avec Luca)
- **Suggestion sous le champ** (maquette B) : au focus d'un champ mot de passe d'inscription, un
  petit menu sous le champ : « Utiliser un mot de passe fort », le mot de passe visible, « MyPass ·
  20 caractères ». Un clic remplit le champ **et** sa confirmation ; Échap ou clic ailleurs ferme.
- **Bannière d'enregistrement à valider d'un clic** (la bannière existante, bouton « Nouveau »).

### Comportement (proposé par Claude ; ★ = choix de Claude non discuté avec Luca)
- **Champ d'inscription** = champ `type=password` qui n'est pas `autocomplete="current-password"`
  et qui remplit l'un de :
  - `autocomplete="new-password"` ;
  - le formulaire (ou le conteneur, hors `<form>`) contient ≥ 2 champs mot de passe — dans un
    formulaire de changement à 3 champs, le premier (actuel) est exclu ;
  - le texte du formulaire, de son bouton d'envoi ou le chemin d'URL contient un mot d'inscription
    (`sign up`, `signup`, `register`, `create account`, `join`, `inscription`, `s'inscrire`,
    `créer un compte`…).
  Un formulaire de connexion classique (un identifiant + un mot de passe, mots de connexion) n'ouvre
  rien.
- La suggestion ne s'affiche que si la base est connectée et déverrouillée. Le mot de passe vient
  de l'action `generate-password` existante, dont l'hôte passe à `mypass-core`
  (`generate_password`, 20 caractères, majuscules/minuscules/chiffres/symboles).
- **Mot de passe généré mémorisé** : au clic sur la suggestion (ou sur l'icône / le menu contextuel
  du générateur), le service worker garde pour l'onglet `{password, username, origine, horodatage}`
  dans `browser.storage.session` (survit à l'arrêt du worker, effacé à la fermeture du navigateur).
  `username` = valeur du champ identifiant/e-mail du formulaire, relue à l'envoi et au `beforeunload`.
- **La bannière s'affiche** quand l'un arrive en premier :
  - l'envoi est détecté (comme aujourd'hui) ;
  - un chargement de page dans le même onglet, sur le même site, alors qu'un mot de passe généré
    ou un envoi est en attente — **même si la page n'a aucun champ et même si l'identifiant est
    vide**.
  ★ Pas d'effacement par le compteur de redirections pour un mot de passe généré. L'attente n'est
  consommée que par un **clic** sur Enregistrer ou Annuler, pas par le simple affichage : une page
  qui redirige aussitôt (/signup → /onboarding) ne doit pas la faire perdre. Tant qu'il n'y a pas eu
  de clic, la bannière revient sur les pages suivantes du même site ; l'attente expire après 15 min
  ou quand l'onglet quitte le site.
- ★ **Identifiants envoyés** (`page.submittedCredentials`) : déplacés aussi dans
  `browser.storage.session`, et `initContentScript` montre la bannière même sans identifiant.
- **Détection d'envoi élargie** sur les formulaires d'inscription : Entrée dans un champ du
  formulaire ; clic sur un `button`, `a`, `[role=button]` ou `input[type=submit]` du formulaire (ou
  du conteneur le plus proche du champ mot de passe) dont le texte est un mot d'envoi/inscription.
- **Entrée créée** : titre = nom d'hôte sans `www.` (`github.com`), pas l'URL complète ; URL = celle
  envoyée par l'extension ; à la racine comme aujourd'hui.
- ★ **L'app se met à jour tout de suite** : après une écriture venue du navigateur (`set-login`,
  `save-api-key`), l'hôte émet un événement Tauri ; le front l'écoute, invalide `entries`/`groups`
  et lance `syncNowAndRefresh`.

## Morceau 2 — Type « Clé API » dans l'app

- `ItemKind` gagne `api_key` ; `MyPass_Type = "api_key"`.
- Champs (présentés à Luca ; ★ `API_URL` en champ personnalisé est un choix de Claude) : `API_Key` (secret, masqué, protégé) et `API_URL` (la page où la clé a
  été créée) ; plus le titre (nom du service, « Mistral ») et les notes.
- ★ **`get-logins` ne renvoie que des identifiants.** Bug existant : dans `url_matches`
  (`commands/browser.rs:262`), `target.contains(&entry_url)` est vrai quand l'URL de l'entrée est
  vide, donc toute entrée sans URL — dont toutes les identités, cartes, documents et clés SSH, créés
  sans URL — est proposée sur **tous** les sites. `handle_get_logins` exclut désormais les entrées
  qui ont un `MyPass_Type` autre que `login`, et `url_matches` ne fait plus correspondre une URL vide.
  Une clé API n'est donc jamais proposée comme mot de passe.
- `API_Key` ajouté à `SECRET_FIELDS` (`src/lib/items.ts`) et à `PROTECTED_CUSTOM_KEYS`
  (`mypass-core/src/ops/entries.rs`) : protégé dans le KDBX, sur desktop et PWA (même core).
- Création à la main : « Nouvelle clé API » dans le menu de la barre latérale, via `ItemForm`
  (`KIND_FIELDS.api_key = ["API_Key", "API_URL"]`), filtre par type, icône (lucide `KeyRound`),
  carte et détail avec copie de la clé, comme les autres types. Traductions EN + FR.
- Import/export JSON : sans perte comme les autres types (champs personnalisés) ; rien de spécial.

## Morceau 3 — Détection des clés API dans l'extension

### Ce qui est décidé (avec Luca)
- Déclenchement à l'**apparition** de la clé dans la page, et au clic sur un bouton « Copier ».
- **Bannière** « Clé API détectée sur `<hôte>` — Enregistrer / Ignorer » ; un clic crée l'élément.
- Si la clé est déjà dans le coffre, **rien ne s'affiche**.

### Comportement (proposé par Claude ; ★ = choix de Claude non discuté avec Luca)
- **Nouveau script de contenu** (`content/apikey.js`, ajouté à la liste `content_scripts` du
  manifeste). Il observe les ajouts au DOM (même mécanisme que `observer-helper.js`) et examine les
  nœuds **visibles** ajoutés : texte, `code`/`pre`, et `input`/`textarea` lus par leur **propriété**
  `.value` (React la pose sans mutation d'attribut). ★ Quand un dialogue apparaît, ses champs sont
  relus quelques fois pendant ~3 s, car la clé arrive souvent après l'appel réseau, dans un champ
  déjà monté. ★ Jamais un champ que l'utilisateur a lui-même tapé ou collé (événements `input` de
  confiance), sinon coller sa clé dans un formulaire la reproposerait ; un champ non readonly rempli
  par la page reste examiné.
- **Candidat** :
  - **préfixe connu** (liste en dur : `sk-`, `sk-proj-`, `sk-ant-`, `ghp_`, `github_pat_`,
    `glpat-`, `hf_`, `gsk_`, `xox[bp]-`, `AIza`, `AKIA`, `sk_live_`/`rk_live_`…) suivi d'au moins
    ~20 caractères → candidat ;
  - **sans préfixe** (clés Mistral : 32 caractères aléatoires) : jeton de 24 à 200 caractères
    `[A-Za-z0-9_\-.]`, sans espace, mêlant lettres et chiffres, d'entropie suffisante, **et** un
    mot-clé dans le même dialogue (`[role=dialog]`) ou à quelques ancêtres près : `api key`,
    `apikey`, `clé api`, `secret`, `token`, `jeton`, `access key`, `copy it now`, `won't be able to
    see`, `ne sera plus affichée`… Exclus : UUID, hex pur de 40 ou 64 caractères (hash de commit,
    SHA-256).
- **Copier** : écoute de `copy` dans le script de contenu, et un petit script injecté dans la page
  (comme `passkeys-inject.js`) qui enveloppe `navigator.clipboard.writeText` et transmet le texte
  au script de contenu (`postMessage`). Le texte copié passe par le même test de candidat
  (le mot-clé n'est pas exigé pour un préfixe connu ; il l'est sinon).
- **Déjà rangée ?** Avant d'afficher, l'extension demande à l'app locale (nouvelle action
  `check-api-key`, canal NaCl chiffré) si la clé est déjà dans le coffre ; rien n'est écrit. Si
  oui : rien ne s'affiche. ★ « Déjà dans le coffre » = n'importe quelle entrée dont un champ
  (mot de passe, `API_Key`, champ personnalisé) vaut exactement la clé, ou dont les notes la
  contiennent — Luca a pu ranger d'anciennes clés en identifiant ou en note. Pas de conversion des
  anciennes entrées.
- **Enregistrer** : nouvelle action `save-api-key` `{key, url, title}` → l'hôte crée l'entrée
  (`MyPass_Type=api_key`, `API_Key` protégé, `API_URL`, titre = nom du service tiré de l'hôte :
  `console.mistral.ai` → `Mistral`), sauvegarde le coffre, émet l'événement de rafraîchissement. Si
  la clé existe déjà entre-temps : pas de doublon, réponse « existe ».
- ★ **Ignorer** : la clé n'est plus proposée de la session (empreinte SHA-256 en
  `browser.storage.session`, jamais la clé en clair).
- ★ **Base verrouillée** au moment où la clé apparaît : la bannière s'affiche quand même (« Clé API
  détectée — déverrouille MyPass pour l'enregistrer ») ; la clé est gardée en mémoire du script de
  contenu de la page, et la bannière passe à « Enregistrer » dès que la base se déverrouille. La
  clé n'apparaissant qu'une fois, ne rien montrer la ferait perdre.
- ★ **Une bannière à la fois** ; une page qui affiche plusieurs candidats propose le premier.

## Contraintes

- Extension : MV3, chargée non empaquetée ; ne pas toucher à la `key` du manifeste ni à
  `allowed_origins` de l'hôte. Les nouvelles actions (`check-api-key`, `save-api-key`) sont
  propres à MyPass : le protocole KeePassXC existant reste inchangé.
- Traductions : app (`src/i18n/en.json`, `fr.json`) et extension (`_locales/en`, `_locales/fr`) ;
  aucune chaîne en dur.
- Ne jamais logger une clé ou un mot de passe (ni `logDebug` côté extension, ni côté hôte).
- Parité desktop/PWA : le type `api_key` s'affiche et s'édite aussi dans la PWA (mêmes composants,
  même core) ; les actions d'hôte sont propres au desktop.
- Livraison : l'hôte change → release desktop (`npm run release`, voir CLAUDE.md) ; l'extension se
  recharge dans `chrome://extensions`.

## Vérification

- Rust : tests unitaires des nouveaux handlers dans `commands/browser.rs` (création `api_key` avec
  `API_Key` protégé, `check-api-key` vrai/faux, pas de doublon, titre tiré de l'hôte pour
  `set-login` et `save-api-key`) ; test de `PROTECTED_CUSTOM_KEYS` pour `API_Key`.
- Extension : la classification « candidat clé API » et « champ d'inscription » en fonctions
  pures, vérifiées par un script Node sans dépendance (comme `src/lib/dedup.check.ts`) : vrais
  positifs (formats connus, clé Mistral dans un dialogue) et faux positifs (hash de commit, UUID,
  jeton CSRF caché, formulaire de connexion).
- **Relevé sur la vraie console Mistral avant d'écrire le détecteur** : Luca crée une clé de test
  dans son Chrome pendant que Claude lit le dialogue (Claude in Chrome) — forme de la clé (texte,
  champ, masquée puis révélée, shadow DOM, iframe), texte et `role` du dialogue, mécanisme du bouton
  Copier (`writeText` ou `execCommand`), longueur et alphabet. La page de test locale « façon
  Mistral » reproduit ce relevé ; Luca supprime ensuite la clé de test. La clé n'est ni loggée ni
  recopiée dans le dépôt.
- Bout en bout dans Chrome avec l'extension rechargée, sur des pages de test locales :
  inscription classique, inscription en JavaScript avec bouton `<div>`, inscription en deux étapes,
  redirection vers une page sans champ, dialogue « votre clé API » façon Mistral avec bouton Copier.
  Puis un essai réel par Luca sur console.mistral.ai.
- Non-régression du CLAUDE.md (trois `cargo test`, clippy, lint, format, build).

## Laissé au constructeur

Seuils exacts (longueur, entropie), liste finale des préfixes et mots-clés, distance de recherche
du mot-clé, style du menu de suggestion (réutiliser `css/autocomplete.css`), nom de l'événement
Tauri, approximation du « même site » pour la bannière différée, découpage des fichiers JS.

## Annexe — relevé console Mistral (2026-11-04)

- La console (`console.mistral.ai/api-keys`, « Vos clés API - Mistral Studio ») est en français pour
  Luca : titre « Vos clés API », bouton « Nouvelle clé ». Les clés existantes sont affichées masquées
  (`**...L24M`) : aucun candidat, donc aucune bannière sur la liste.
- Sur le plan Free, « Nouvelle clé » est désactivé : impossible de créer une clé de test, le dialogue
  de création n'a pas pu être relevé. Ne pas utiliser « régénérer » (↻) sur une clé existante :
  cela casserait ce qui l'utilise.
- Décision (Luca, 2026-11-04) : le relevé est remplacé par un essai réel pendant la vérification de
  bout en bout, avec l'extension modifiée, sur un jeton GitHub (Settings → Developer settings →
  Personal access tokens : affiché une seule fois, bouton Copier), supprimé juste après. Le cas
  « clé sans préfixe façon Mistral » reste couvert par la page de test locale.
