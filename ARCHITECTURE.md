# Architecture d'UltraRushFTP

Objectif : toutes les fonctionnalités de FTPRush v2, modernisées, avec le fonctionnement
**par onglets** (containers) et la compatibilité `rushmirc.dll`. FTPRush v3 ne sert de
référence que pour les types de connexion.

## Crates

| Crate | Rôle |
|---|---|
| `urftp-core` | Modèle : sites, signets, onglets, file d'attente, skiplist, priorités, statistiques |
| `urftp-proto` | FTP, FTPS explicite/implicite, SFTP, FXP (dont SSCN/CPSV) |
| `urftp-script` | Interpréteur du Pascal Script de FTPRush (`RushApp.*`, constantes `RS_*`) |
| `urftp-ipc` | Côté application du protocole rushmirc (fenêtre `TfmRush`, mémoire `FTPRush`, `WM_COPYDATA`) |
| `rushmirc` | DLL mIRC compatible (`Invoke`, `RushScript`, `RushCommand`, callbacks) |
| `urftp-cli` | Ligne de commande `urftp` |
| `urftp-gui` | Interface Windows `UltraRushFTP.exe` |

Dépendances : `gui` → `core`, `proto`, `script`, `ipc` ; `cli` → `core`, `proto` ;
`rushmirc` → `ipc` seulement (la DLL reste légère et ne charge pas le moteur).

## Feuille de route

### Phase 1 — protocoles et moteur
1. Client FTP maison (contrôle complet des commandes, parseurs LIST Unix/DOS, MLSD).
2. FTPS explicite (`AUTH TLS`, `PBSZ`, `PROT`) et implicite ; TLS sur le canal de données.
3. FXP : `PASV`/`PORT` croisés, `PRET`, FXP sécurisé `SSCN` / `CPSV`, inversion automatique.
4. SFTP : mot de passe et clé publique.
5. File d'attente : upload / download / FXP, reprise, priorités, skiplist, transferts simultanés,
   limites de logins par site, limitation de bande passante.

### Phase 2 — application
6. Interface à onglets : chaque onglet = deux panneaux (local ou distant) + sa file.
7. Gestionnaire de sites (groupes, signets, proxy, ident, keep-alive, ghost/`!user`).
8. Journaux (FTP, transferts, système), barre d'état, statistiques.
9. Thèmes clair/sombre, raccourcis, actions de souris, langues.

### Phase 3 — automatisation et compatibilité
10. Pascal Script (`RushApp.FTP.Login/Logout/CWD/RAW/Delete/Transfer/RemoveQueue/…`).
11. Serveur IPC + DLL `rushmirc` (x86 et x64), test contre les scripts mIRC existants.
12. Ligne de commande (`-d`, `-w`, `-c`, `-f`, `-r[t][x]`) et `urftp`.
13. Import des sites FTPRush v2 (`RushSite.xml`) et v3 (`site.json`).

### Plus tard
Protocoles cloud (WebDAV, S3, Google Drive, OneDrive, Dropbox) : hors périmètre initial.

## Référence fonctionnelle FTPRush v2

Pages d'aide de la v2 à couvrir (parité) : site manager, quick connect, signets/favoris,
file d'attente (édition, priorités, échecs, sauvegarde/chargement, planification),
skiplist / allowlist / priorités, transfert conditionnel, recherche rapide et recherche
sur sites, comparaison de répertoires, CRC/SFV/MD5, commandes brutes et designer de
commandes, éditeur de proxy, ident, UPnP, keep-alive, notifications d'événements,
gestionnaire de raccourcis, actions de souris, layouts, couleurs/thèmes, statistiques,
export d'arborescence, calcul d'espace utilisé, verrouillage dans la zone de notification,
actions de fin de file (veille, hibernation, arrêt).
