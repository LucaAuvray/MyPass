# Agent SSH intégré — Design

**Date :** 2026-07-08
**Statut :** validé par Luca

## Objectif

MyPass stocke des clés SSH dans le coffre KDBX et sert d'agent SSH sous Windows,
comme 1Password : `ssh user@machine` dans un terminal ou VS Code Remote-SSH
utilise les clés du coffre automatiquement, avec une popup de confirmation à
chaque demande de signature. La clé privée ne quitte jamais le process MyPass.

## Approche retenue

Agent intégré au process Tauri existant (pas de binaire séparé). Une tâche tokio
écoute le named pipe Windows `\\.\pipe\openssh-ssh-agent` — le pipe par défaut
du client OpenSSH de Windows (`ssh.exe`), utilisé aussi par VS Code Remote-SSH.
Grâce à `tauri-plugin-single-instance`, le coffre déverrouillé et l'agent vivent
dans le même process : lecture des clés en mémoire, popup de confirmation via
une fenêtre Tauri, zéro IPC supplémentaire.

Approches écartées : binaire séparé `--ssh-agent` (IPC chiffré inutile pour
transporter des clés privées entre process) ; pont Pageant/KeeAgent (protocoles
legacy PuTTY, inutiles avec l'OpenSSH natif de Windows).

## Crates

- `ssh-agent-lib` — protocole ssh-agent côté serveur, support des named pipes Windows.
- `ssh-key` (RustCrypto, feature `encryption`) — parsing des clés OpenSSH
  (Ed25519, RSA, ECDSA), déchiffrement des clés à passphrase, signature.

## 1. Stockage

Nouveau type d'entrée « Clé SSH » dans le KDBX, même pattern que
identités/cartes/documents :

- Champ protégé : clé privée au format OpenSSH PEM (stockée **déchiffrée** —
  si le fichier importé a une passphrase, elle est demandée à l'import ; c'est
  le coffre qui protège ensuite).
- Champs normaux : clé publique, fingerprint SHA256, commentaire, type de clé.

Deux entrées possibles dans le coffre :

- **Import** d'un fichier existant (`~/.ssh/id_*`), avec saisie de passphrase
  si nécessaire.
- **Génération** d'une clé Ed25519 dans l'app (bouton copier la clé publique
  pour la déposer sur les serveurs).

## 2. Agent

- Démarre quand le coffre est déverrouillé **et** que le toggle « Agent SSH »
  des réglages est actif (préférence persistante). S'arrête au verrouillage du
  coffre ; les demandes sont alors refusées proprement (échec de signature,
  pas de crash côté client SSH).
- `request_identities` (`ssh-add -l`) liste toutes les clés SSH du coffre.
- `sign_request` déclenche une popup de confirmation : nom de la clé,
  fingerprint, boutons Autoriser / Refuser. Timeout sans réponse = refus.
  Option « se souvenir pour cette session de déverrouillage » (par clé,
  réinitialisée au verrouillage).
- Les opérations d'écriture du protocole (`add_identity`, `remove_identity`)
  sont refusées : le coffre est la seule source de vérité.

## 3. Réglages et diagnostic

Section « Agent SSH » dans les réglages, même pattern que l'intégration
navigateur :

- Toggle activer/désactiver + état courant du pipe (actif / inactif / conflit).
- Détection du service Windows « OpenSSH Authentication Agent » (qui occupe le
  pipe s'il tourne) : bouton « Désactiver le service Windows » qui lance la
  commande avec élévation UAC (PowerShell `Start-Process -Verb RunAs`).
- Texte d'aide : vérifier avec `ssh-add -l` dans un terminal.

## 4. Erreurs

- Pipe déjà occupé au démarrage de l'agent → état « conflit » dans les
  réglages, message pointant vers le bouton de désactivation du service.
- Clé importée illisible / passphrase incorrecte → erreur explicite à l'import.
- Demande de signature pour une clé absente du coffre → refus protocolaire.

## 5. Tests

- Tests unitaires Rust embarqués (`#[cfg(test)]`, comme `kdbx/crypto.rs`) :
  parsing de clés (Ed25519, RSA, avec/sans passphrase), génération,
  round-trip signature/vérification.
- Validation E2E manuelle : `ssh-add -l`, connexion `ssh` réelle depuis un
  terminal, et VS Code Remote-SSH — popup de confirmation à l'appui.

## Hors périmètre (v1)

- macOS/Linux (socket Unix) — l'app n'est utilisée que sous Windows aujourd'hui.
- Forwarding d'agent, certificats SSH, clés FIDO2/sk-*.
- Import de formats non-OpenSSH (PuTTY .ppk).
