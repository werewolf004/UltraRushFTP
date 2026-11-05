# Journal des changements

[English](CHANGELOG.md) | **Français**

Les changements notables d'UltraRushFTP. Le format suit
[Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) et le projet utilise le
[versionnage sémantique](https://semver.org/lang/fr/). Les notes des versions publiées sur
GitHub reprennent la version anglaise ([CHANGELOG.md](CHANGELOG.md)) ; ce fichier en est
la traduction.

## [Non publié]

## [0.1.0] - 2026-11-05

Première version : moteur de protocoles, interface à onglets, gestionnaire de sites,
options, scripts, liaison mIRC et ligne de commande. Pas encore validée sur des serveurs
réels.

### Ajouté

- **Client FTP** écrit pour le projet : contrôle complet des commandes, analyse de `LIST`
  (Unix, DOS/IIS) et `MLSD`, repli `EPSV` → `PASV`, `PRET`, reprise `REST`, chemins locaux
  et distants longs (repli `CWD` segment par segment), injection CRLF refusée, mot de passe
  masqué dans les journaux.
- **FTPS** explicite (`AUTH TLS`, repli `AUTH SSL`) et implicite, `PBSZ`/`PROT`, reprise de
  session TLS sur les canaux de données ; certificats vérifiés avec le magasin Windows,
  approuvés par empreinte SHA-256 ou acceptés ; mode actif `PORT`/`EPRT` avec IP externe et
  plage de ports.
- **Proxys** SOCKS4, SOCKS4A, SOCKS5 et tunnel HTTP pour les canaux de contrôle et de
  données, avec règles de contournement ; **serveur Ident** (RFC 1413, `URUSHFTP` par
  défaut).
- **FXP** dans les deux ordres de rôles, chiffré par `SSCN`/`CPSV`, inversion automatique
  si `PORT` est refusé, interruption propre et restauration de `PROT`/`SSCN`.
- **SFTP** par mot de passe ou clé (OpenSSH, PEM, PuTTY `.ppk`), clés d'hôte approuvées par
  empreinte, reprise, requêtes en pipeline, proxy.
- **Moteur de file** : plusieurs transferts en parallèle par site, limites par site et
  globale, dossiers développés à leur tour, liste d'exclusion, priorités, règles « fichier
  existant », nouvelles tentatives, arrêt, limitation de débit.
- **Interface** à onglets (« containers » de FTPRush) : deux panneaux de fichiers, file,
  journal et console de script dans un dock, connexion rapide, glisser-déposer, raccourcis
  clavier, dialogues d'approbation des certificats et des clés SSH, thèmes sombre et clair.
- **Gestionnaire de sites** avec dossiers imbriqués, fiches complètes, signets, mots de
  passe chiffrés par DPAPI, import depuis FileZilla, FTPRush v1/v2/v3 et FlashFXP (les mots
  de passe de FTPRush v1/v2 et FlashFXP sont à ressaisir).
- **Fenêtre Options** : transferts, débit, fichier existant, liste d'exclusion, priorités,
  proxy global, Ident, thème, langue ; enregistrement automatique.
- Interpréteur **Pascal Script** et API `RushApp.*` (`Login`, `Logout`, `CWD`, `RAW`,
  `Delete`, `Transfer`, `RemoveQueue`, `EnableSite`, `DisableSite`, statistiques…).
- **Liaison mIRC** : fenêtre cachée `TfmRush` et `rushmirc.dll` (32 et 64 bits) compatible
  avec les scripts FTPRush, réponses des `RAW` affichées dans mIRC.
- **Ligne de commande** : options de démarrage de FTPRush v2 (`-w`, `-c`, `-f`,
  `-r[t][x]`, `-d`) plus `-s` (script au démarrage) ; outil `urftp` (`sites`, `ls`, `get`,
  `put`, `fxp`, `raw`).
- **Langues** : interface et ligne de commande en anglais et en français, langue de
  Windows par défaut.
- **Documentation** en anglais et en français avec captures, références des options et
  des dépendances ; licence propriétaire.
- **CI/CD** : formatage, cargo-deny, Clippy, tests, documentation, vérification des exports
  des DLL 32/64 bits à chaque push ; versions taguées avec binaires, sommes de contrôle et
  mentions des licences.

### Sécurité

- Code `unsafe` limité à `urftp-sys`, avec une justification documentée pour chaque bloc ;
  `unwrap`, `expect`, `panic`, `unreachable` interdits dans le code de production.
- Alerte connue acceptée : `RUSTSEC-2023-0071` dans `rsa` (utilisée pour les clés SSH
  RSA) ; voir [SECURITY.fr.md](SECURITY.fr.md).

[Non publié]: https://github.com/werewolf004/UltraRushFTP/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/werewolf004/UltraRushFTP/releases/tag/v0.1.0
