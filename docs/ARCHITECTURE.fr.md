# Architecture d'UltraRushFTP

[English](ARCHITECTURE.md) | **Français**

Objectif : toutes les fonctionnalités de FTPRush v2, modernisées, avec les **onglets** de
sites (containers) et la compatibilité `rushmirc.dll`. FTPRush v3 ne sert de référence que
pour les types de connexion.

## Crates

| Crate | Rôle | `unsafe` |
|---|---|---|
| `urftp-core` | Modèle : sites et signets, enregistrement du carnet, options, file d'attente et fichiers de file, options d'élément des scripts et du transfert conditionnel (`itemopts`), liste d'exclusion, priorités, règles « fichier existant », sommes SFV / MD5, aides des panneaux (`browsing` : filtre rapide, masques, renommage de plusieurs éléments, comparaison de dossiers, parcours locaux, espace utilisé, arborescence, modes CHMOD, dates), adresses et listes incrémentées (`urls`), imports et exports, empreinte du mot de passe maître (`master`, PBKDF2 de `ring`), sauvegardes de la configuration (`backup`), secrets protégés par DPAPI | interdit |
| `urftp-proto` | Client FTP, FTPS explicite/implicite, SFTP, FXP, proxys, serveur Ident, redirection de ports UPnP (`upnp` : SSDP et SOAP), moteur de file, limitation de débit, conversion fiche de site → profil de connexion | interdit |
| `urftp-script` | Interpréteur du Pascal Script de FTPRush (procédures, tableaux, `case` ; `RushApp.*`, drapeaux `RS_*`, gestionnaires d'événements) | interdit |
| `urftp-i18n` | Catalogues anglais et français (`locales/*.json`), macro `t!`, détection de la langue de Windows | interdit |
| `urftp-sys` | **Seule crate autorisée à contenir du `unsafe`** : fenêtre de la liaison mIRC (`TfmUltraRush`, ou `TfmRush` en mode compatibilité) et mémoire partagée (côté application du protocole mIRC), DPAPI, veille / veille prolongée / arrêt (`power`), espace disque libre, fichiers copiés dans l'Explorateur (`CF_HDROP`), texte du presse-papiers, état de la touche `Ctrl+V`, heure locale, icône de la zone de notification, clignotement de la fenêtre et sons des événements (`tray`), instance unique (`WM_SHOW_APP`), exports de `rushmirc.dll` (fonctionnalité `mirc-dll`) | autorisé, audité |
| `rushmirc` | DLL mIRC (`cdylib`) : réexporte `urftp_sys::mirc`, sans code propre | interdit |
| `urftp-cli` | Outil en ligne de commande `urftp` | interdit |
| `urftp-gui` | Interface `UltraRushFTP.exe` (egui / eframe, egui_dock) | interdit |

```text
urftp-gui ──► urftp-core, urftp-proto, urftp-script, urftp-sys, urftp-i18n
urftp-cli ──► urftp-core, urftp-proto, urftp-i18n
urftp-proto ─► urftp-core ─► urftp-sys (DPAPI)
rushmirc ───► urftp-sys seulement (la DLL reste légère et ne charge jamais le moteur)
```

## Règles imposées par le compilateur

Déclarées une fois dans `[workspace.lints]` (`Cargo.toml` racine) et héritées par toutes
les crates :

- `unsafe_code = "forbid"` partout sauf dans `urftp-sys`, qui a ses propres règles, plus
  strictes : `undocumented_unsafe_blocks` et `missing_safety_doc` refusés (chaque bloc
  `unsafe` porte une justification `// SAFETY:`), `unsafe_op_in_unsafe_fn` refusé.
- `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `unreachable`,
  `dbg_macro` refusés hors tests (`clippy.toml` ne les autorise que dans les tests).
- Erreurs typées avec `thiserror` (`FtpError`, `SftpError`, `SessionError`, `ProxyError`,
  `ScriptError`, `HostError`, `ImportError`, `CliError`…) ; les remarques destinées à
  l'utilisateur sont typées aussi (`ImportWarning`, `SkipReason`) pour que l'interface les
  traduise.
- Formatage par `rustfmt.toml` ; Clippy tourne avec `-D warnings` dans la CI.

## Conception

### Fils d'exécution et circulation des données

- L'interface (egui) tourne sur le fil principal et ne bloque jamais sur le réseau. Chaque
  panneau distant possède un *acteur* : une tâche tokio qui tient la connexion de
  navigation, pilotée par des messages `BrowseCmd` et qui répond par des `BrowseEvent`
  (`urftp-proto::browse`, relayée vers le fil de l'interface par `urftp-gui::browser`).
  Inactive, elle envoie les commandes anti-inactivité de `urftp-proto::keepalive` ; quand
  la connexion est perdue, elle se reconnecte et revient dans son dossier.
- Chaque script tourne dans un fil à lui (`urftp-gui::app::scripting`). Ses appels
  `RushApp.*` vont au fil de l'interface, qui y répond entre deux images (jusqu'à 15 ms par
  image pour un script chargé) ; `InputQuery` et la confirmation de `ShellExecuteCommand`
  sont des boîtes de dialogue auxquelles on répond plus tard, et Arrêter fait échouer
  l'appel suivant. `Invoke` (mIRC) est évalué aussitôt sur le fil de l'interface.
- Les transferts sont exécutés par le moteur de file (`urftp-proto::engine`) sur un runtime
  tokio multi-fil séparé. Chaque onglet a sa `Queue` ; le moteur garde un réservoir de
  connexions par site et publie des `EngineEvent` (démarré, progression, terminé, ignoré,
  échec…). Il se met en pause (les éléments en cours attendent à nouveau) et s'arrête pour
  de bon (*Tout arrêter*) ; les dossiers sont développés avec les options de leur élément
  (filtres, profondeur, tri, relectures, synchronisation avec la liste de la cible), et les
  transferts de fichiers suivent `TransferRules` (ASCII, `MODE Z`, noms, espace disque,
  réseau local, `XDUPE`, contrôles SFV / `XCRC`).
- Toute connexion (panneau, moteur, test de disponibilité) passe par
  `RemoteSession::open`, qui essaie à tour de rôle les autres adresses du site, et ouvre
  sa socket TCP avec `urftp-proto::net` (« happy eyeballs » IPv6/IPv4, RFC 8305).
- `App::logic` s'exécute à chaque image, y compris fenêtre réduite : transferts,
  statistiques et demandes de mIRC continuent d'être traités.

### Chemins longs

- **Local** : aucune limite `MAX_PATH` (260). Les opérations sur les fichiers passent par
  `std::fs`, qui ajoute le préfixe `\\?\` quand il le faut ;
  `urftp-core::localpath::to_verbatim` produit des chemins `\\?\C:\…` / `\\?\UNC\…` pour
  les appels Win32 directs. Testé avec des arborescences de plus de 600 caractères. Le
  manifeste d'application déclare `longPathAware` : les appels Win32 acceptent aussi les
  chemins longs quand ils sont activés dans Windows.
- **Distant** : aucun tampon de taille fixe. Quand un serveur refuse un `CWD` absolu trop
  long, le client navigue segment par segment, puis agit sur le nom seul.

### Onglets et multi-transfert

- Un **onglet** est un contexte indépendant : deux panneaux, sa file, son journal, sa
  console de script. Plusieurs onglets peuvent être connectés à des sites différents ou au
  même site.
- **Multi-transfert** : d'un même onglet vers un même site, plusieurs fichiers sont
  transférés en parallèle sur des connexions de transfert distinctes de la connexion de
  navigation, dans les limites du site (transferts simultanés, téléchargements maximum,
  envois maximum ; 0 = connexion unique) et d'une limite globale facultative.
- Ordre de la file : priorités, puis ordre d'ajout ; les dossiers sont développés quand
  leur tour arrive et leur contenu prend leur place.

### Connexions

- **FTP**, **FTPS explicite** (`AUTH TLS`, repli `AUTH SSL`), **FTPS implicite**,
  `PBSZ 0` + `PROT P|C` par site, reprise de session TLS sur les canaux de données (exigée
  par vsftpd `require_ssl_reuse`, glftpd…), **SFTP** (russh, fournisseur ring, 16 requêtes
  en vol).
- **FXP** : `PASV` côté source / `PORT` côté destination ou l'inverse, `PRET`, `REST`, FXP
  chiffré par `SSCN` ou `CPSV` (repli en clair facultatif), inversion automatique des rôles
  si `PORT` est refusé, `ABOR` de l'autre côté en cas d'échec, `PROT`/`SSCN` restaurés
  ensuite.
- **Proxy** (global ou par site) : SOCKS4, SOCKS4A, SOCKS5 (avec authentification), tunnel
  HTTP (`CONNECT`, authentification Basic) ; facultatif pour les canaux de données ;
  contournement des adresses locales et par masques d'hôtes.
- **Ident** (RFC 1413) : serveur sur TCP 113 (modifiable), nom global (`URUSHFTP` par
  défaut), système « UNIX ».

### Connexion de navigation et outils de dossier

- Chaque panneau distant a une connexion de navigation (`urftp_proto::browse`) : un acteur
  qui exécute les commandes du panneau l'une après l'autre (lister, changer de dossier,
  renommer, `SITE CHMOD`, `MFMT` / `SITE UTIME`, listing brut), maintient la connexion et se
  reconnecte. Après une série d'opérations, il revient dans le dossier du panneau, car le
  parcours du contenu d'un dossier change le dossier courant FTP.
- Le cache des dossiers vit dans cet acteur (par connexion, par dossier) ; une
  actualisation relit toujours le serveur, et l'interface lui fait oublier les dossiers
  dans lesquels un transfert a écrit.
- Les parcours plus longs (espace utilisé, arborescence, recherche sur les sites) et les
  fichiers ouverts avec un autre programme ont leurs propres connexions
  (`RemoteSession::walk`, `retr`, `stor`) : le panneau reste libre ; leurs résultats
  reviennent à l'interface par des canaux. Les parcours locaux tournent sur un fil
  (`urftp_core::browsing::walk_local`).

### Protocole mIRC (`rushmirc.dll`)

Notes tirées de la source Delphi d'origine (non redistribuée dans ce dépôt) :

| Élément | Valeur |
|---|---|
| Classe de la fenêtre principale cherchée par la DLL | `TfmUltraRush`, puis `TfmRush` (FTPRush, ou UltraRushFTP en mode compatibilité) ; `URUSHFTP_IPC_CLASS` remplace les deux pour les tests |
| Mémoire partagée application ↔ DLL | `FTPRush`, 4096 octets |
| Mémoire partagée DLL → mIRC | `mIRC`, 4096 octets, puis `WM_USER + 200` |
| `Invoke` | copie la requête dans `FTPRush`, envoie `WM_USER + 300` (`WM_EXCHANGE`), relit la réponse |
| `RushScript` | `WM_COPYDATA`, `dwData = 1000`, `wParam` = fenêtre de rappel si les rappels sont actifs |
| `RushCommand` | `WM_COPYDATA`, `dwData = 1001` |
| Réponse d'une commande brute | `WM_COPYDATA` vers la fenêtre de rappel, `wParam = 888`, `dwData` 1 = succès / 0 = échec ; lignes : site, commande, résultats |

L'original définit `EnableCallback` / `DisableCallback` sans les exporter ; UltraRushFTP
les exporte. La CI vérifie que les deux DLL (32 et 64 bits) exportent exactement
`LoadDll`, `UnloadDll`, `Invoke`, `RushScript`, `RushCommand`, `EnableCallback` et
`DisableCallback`.

### Données et secrets

- Dossier des données : `%APPDATA%\UltraRushFTP`, ou le dossier donné par `-f` / `--data`
  / `URUSHFTP_DATA`. Fichiers : `sites.json` (écriture atomique, copie `sites.json.bak`),
  `settings.json`, `stats.json`.
- Les mots de passe (sites, proxys) sont enregistrés sous la forme `dpapi:<base64>` :
  DPAPI de Windows, lié au compte de l'utilisateur, avec une entropie propre à
  l'application. Un fichier copié sur une autre machine ou un autre compte ne révèle rien.
  Détails dans [SECURITY.fr.md](../SECURITY.fr.md).

## Tests

- Tests unitaires à côté du code ; tests d'intégration dans `crates/urftp-proto/tests`
  contre des serveurs FTP/FTPS, SFTP, proxys SOCKS et HTTP en mémoire (localhost
  seulement), et un test mIRC de bout en bout (fausse fenêtre mIRC ↔ DLL ↔ serveur
  `TfmRush`) dans `crates/urftp-sys`.
- `cargo run -p urftp-proto --example dev_server` démarre le même serveur FTP pour les
  essais manuels (`test` / `secret`).
- `scripts/screenshots.ps1` régénère les images de la documentation à partir de données de
  démonstration. L'application est dessinée hors écran par wgpu (le test ignoré
  `app::shots::render_screenshot` de `urftp-gui`, avec `egui_kittest`) : aucune fenêtre ne
  s'ouvre et rien ne prend le focus.

## Référence fonctionnelle : FTPRush v2

Pages d'aide de la v2 à couvrir (parité) : gestionnaire de sites, connexion rapide,
signets/favoris, file d'attente (édition, priorités, échecs, sauvegarde/chargement,
planification), liste d'exclusion / liste d'autorisation / priorités, transfert
conditionnel, recherche rapide et recherche sur les sites, comparaison de répertoires,
CRC/SFV/MD5, commandes brutes et concepteur de commandes, éditeur de proxy, Ident, UPnP,
keep-alive, notifications d'événements, gestionnaire de raccourcis, actions de souris,
dispositions, couleurs/thèmes, statistiques, export d'arborescence, calcul de l'espace
utilisé, verrouillage dans la zone de notification, actions de fin de file (veille,
hibernation, arrêt). L'avancement est suivi dans [ROADMAP.fr.md](../ROADMAP.fr.md).
