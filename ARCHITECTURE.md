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

## Exigences transverses (non négociables)

### Chemins longs
- **Local** : aucune limite `MAX_PATH` (260). Toutes les opérations fichiers passent par
  `std::fs` (qui préfixe `\\?\` automatiquement) ou par un helper qui normalise en chemin
  verbatim `\\?\C:\…` / `\\?\UNC\serveur\partage\…` pour les appels Win32 directs.
  Manifeste de l'exe avec `longPathAware = true`. Tests avec des arborescences > 260 et > 1024 caractères.
- **Distant** : aucun tampon de taille fixe pour les chemins FTP/SFTP. Si un serveur refuse
  un `CWD`/`STOR`/`RETR` en chemin absolu trop long, repli automatique sur une navigation
  pas à pas (`CWD` segment par segment) puis commande en nom relatif.

### Onglets et multi-transfert
- Un **onglet** = un contexte indépendant (deux panneaux, sa file, ses journaux) ; plusieurs
  onglets peuvent être connectés à des sites différents, ou au même site.
- **Multi-transfert** : depuis un même onglet vers un même site, plusieurs fichiers partent
  en parallèle sur des connexions de transfert distinctes de la connexion de navigation.
  Pool de connexions par site avec limites : transferts simultanés, logins max, logins max
  en download / upload (comme la v2). Valeur 0 = mode connexion unique (la connexion de
  navigation transfère).
- Ordre de file respecté : priorités, puis ordre d'ajout ; option « dossier n+1 ne démarre
  pas avant la fin du dossier n » (`RS_NOCIRCLE`).

### Connexions
- **FTP**, **FTPS explicite** (`AUTH TLS`, repli `AUTH SSL`), **FTPS implicite** (port 990),
  TLS sur le canal de données (`PBSZ 0` + `PROT P`) activable par site, **SFTP**.
- **FXP** : ordre normal (source `PASV`, cible `PORT`) et options inverses de la v2 (« pas de
  PASV côté source », « PASV côté cible »), inversion automatique, FXP chiffré `SSCN` / `CPSV`,
  `PRET` avant `PASV` quand le serveur l'annonce.
- **Proxy** (global, par site, ou contournement) : SOCKS4, SOCKS4A, SOCKS5 (avec
  authentification), HTTP Tunnel (`CONNECT`). Option « appliquer au canal de données »,
  exceptions par masque, contournement des adresses locales, NOOP périodique.
- **Ident** (RFC 1413) : serveur sur TCP 113 (port configurable), nom global (« URUSHFTP » par
  défaut ; « Rush » dans FTPRush v2) ou par site, système « UNIX », option « actif seulement pendant la connexion ».
- Liste d'IP par site avec rotation en cas d'échec de login, reconnexion si expulsé,
  keep-alive, ghost login (`!user`).

### Règles de transfert
- **Skiplist / fichiers ignorés** : masques joker et regex, `%W%` pour le chemin complet,
  listes séparées upload / download / FXP, globales et par site ; options « ignorer les
  fichiers de 0 octet », « ignorer les dossiers vides », « appliquer aux dossiers »,
  mode auto / manuel / les deux, couleur des éléments ignorés.
- **Priorités** : liste ordonnée de masques ; les éléments correspondants passent en tête de file.
- **Fichier existant** : reprise / ignorer / renommer / écraser / demander, par cas
  (cible plus petite, identique, plus grande), global et par site ; écraser si la reprise échoue.
- **XDupe**, conversion de casse des noms, mode actif forcé, taille de tampon TCP.

## Feuille de route

### Phase 1 — protocoles et moteur
1. ✅ Client FTP maison (contrôle complet des commandes, parseurs LIST Unix/DOS, MLSD),
   chemins longs locaux et distants dès le départ.
2. ✅ FTPS explicite (`AUTH TLS`, `PBSZ`, `PROT`) et implicite ; TLS sur le canal de données
   avec reprise de session ; certificats : magasin Windows, empreinte approuvée, ou sans
   vérification ; mode actif `PORT`/`EPRT` (IP externe, plage de ports).
3. ✅ Proxy (SOCKS4/4A/5 avec authentification, HTTP Tunnel avec Basic) sur le contrôle et
   les données, contournement local et par masques, passage auto en passif ; serveur Ident
   (RFC 1413) avec nom par site, nom global `URUSHFTP` par défaut.
4. ✅ FXP : `PASV`/`PORT` croisés (source ou destination passive), `PRET`, reprise `REST`,
   FXP chiffré `SSCN` / `CPSV` (repli en clair optionnel), inversion automatique si `PORT`
   est refusé, `ABOR` de l'autre côté en cas d'échec, restauration de `PROT`/`SSCN`.
5. ✅ SFTP (russh, fournisseur ring) : mot de passe ou clé (OpenSSH, PEM, PuTTY `.ppk`,
   chiffrée ou non), clé d'hôte vérifiée par empreinte `SHA256:…`, reprise, proxy,
   16 requêtes en vol par défaut (pipelining, réglable).
6. ✅ File d'attente (`urftp-core::queue` + `urftp-proto::engine`) : upload / download / FXP,
   réservoir de connexions et multi-transfert par site (limites transferts / téléchargements
   / envois, 0 = connexion unique), dossiers développés à leur tour, skiplist ordonnée
   (joker, regex, `%W%`, sens, 0 octet, dossiers vides), priorités, règles « fichier
   existant » (reprendre / ignorer / renommer / écraser / demander), repli si `REST` refusé,
   nouvelles tentatives, arrêt d'un élément, limitation de débit, événements pour l'interface.
   Restent pour la phase 3 : `RS_NOCIRCLE`, répétitions / `CompleteFlag`, XDupe.

### Phase 2 — application
7. 🟡 Interface à onglets (egui 0.36 + egui_dock 0.21) : thème moderne clair/sombre à coins
   arrondis, polices Segoe UI ; onglets de sites ; dans chaque onglet un dock (deux panneaux
   local/distant, file d'attente, journal) déplaçable ; connexion rapide FTP/FTPS/SFTP ;
   approbation des certificats TLS et clés SSH ; transfert de la sélection vers l'autre
   panneau et file branchée sur le moteur ; clic droit (transférer, ouvrir, renommer,
   supprimer avec confirmation, nouveau dossier, copier le chemin), glisser-déposer entre
   panneaux, tri des colonnes, sélection Ctrl/Maj/Ctrl+A, raccourcis (Entrée, Retour
   arrière, F2, F5, F7, Suppr, Espace/Inser, Tab), contrôle de la file (arrêter, relancer,
   monter/descendre, retirer, tout arrêter, relancer les échecs).
8. 🟡 Gestionnaire de sites (`urftp-core::site` / `sitestore`, fenêtre `urftp-gui::sites`) :
   dossiers imbriqués, fiche complète (protocole, TLS/empreinte, clé SSH et clés d'hôte,
   mode passif/actif, limites de connexions, proxy, Ident, signets, notes, activé),
   enregistrement `%APPDATA%\UltraRushFTP\sites.json` (ou `URUSHFTP_DATA`) avec mots de
   passe chiffrés DPAPI, empreintes approuvées mémorisées. Import (`urftp-core::import`,
   détection auto) : FileZilla ✅, FTPRush v3 ✅, FTPRush v1/v2 ✅ et FlashFXP ✅ (pour ces
   deux derniers, mots de passe non importés : à ressaisir). Reste : keep-alive, ghost/`!user`, liste d'IP, skiplist par site.
9. Journaux (FTP, transferts, système), barre d'état, statistiques.
10. Thèmes clair/sombre, raccourcis, actions de souris, langues.

### Phase 3 — automatisation et compatibilité
11. Pascal Script (`RushApp.FTP.Login/Logout/CWD/RAW/Delete/Transfer/RemoveQueue/…`).
12. Serveur IPC + DLL `rushmirc` (x86 et x64), test contre les scripts mIRC existants.
13. Ligne de commande (`-d`, `-w`, `-c`, `-f`, `-r[t][x]`) et `urftp`.
14. Import des sites FTPRush v2 (`RushSite.xml`) et v3 (`site.json`).

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
