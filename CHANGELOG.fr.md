# Journal des changements

[English](CHANGELOG.md) | **Français**

Les changements notables d'UltraRushFTP. Le format suit
[Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) et le projet utilise le
[versionnage sémantique](https://semver.org/lang/fr/). Les notes des versions publiées sur
GitHub reprennent la version anglaise ([CHANGELOG.md](CHANGELOG.md)) ; ce fichier en est
la traduction.

## [Non publié]

### Ajouté

- **Validation sur de vrais serveurs** : `scripts/validate-server.ps1` déroule un scénario
  fixe avec `urftp` (liste, envoi et téléchargement d'un dossier en plusieurs transferts,
  reprise dans les deux sens, écrasement, FXP, suppression), compare chaque fichier par
  SHA-256 et écrit un rapport anonymisé ; guide et liste de contrôle par serveur dans
  [docs/VALIDATION.fr.md](docs/VALIDATION.fr.md). Le script est aussi dans l'archive de la
  version.
- **`urftp -v` journalise aussi les connexions de transfert**, pas seulement la première
  connexion.
- **File non terminée gardée d'un lancement à l'autre** : enregistrée automatiquement
  (`queue.json`) et proposée au démarrage suivant (Options › Général : demander, charger ou
  abandonner).
- **Compatibilité avec la DLL mIRC d'origine de FTPRush** (Options › Général), désactivée
  par défaut.
- **Proxys nommés** (Options › Proxy), choisis par leur nom dans les fiches de site.
- **`NOOP` pendant les transferts** (Options › Connexion), pour les proxys et routeurs qui
  coupent une connexion de contrôle inactive.
- **Listings chiffrés ou non, indépendamment des transferts** (fiche d'un site FTPS).
- **Ident seulement pendant la connexion** (Options › Ident).
- **Plusieurs clés SSH par site**, essayées à tour de rôle.
- **Rôles FXP par site** : jamais `PASV` comme source, toujours `PASV` comme cible.
- **Description du site**, **format du listing** (automatique, Unix, DOS/IIS) et **adresse
  web** d'un site, avec *Copier l'adresse HTTP* dans le panneau distant.

- **Réglages de listing** (Options › Connexion, et par site) : fichiers cachés (`LIST -a`),
  `MLSD` activé ou non, commande de listing personnalisée, `STAT -l` sur le canal de
  contrôle, retrait des espaces de tête, passage en mode actif quand le mode passif échoue.
- **Décalage horaire et jeu de caractères par site** : dates des listings décalées ; noms,
  chemins et réponses convertis pour les serveurs qui n'utilisent pas UTF-8.
- **Mode actif et réseau** (Options › Connexion) : adresse externe annoncée par `PORT`,
  plage de ports locaux, adresse locale d'où partent les connexions.
- **E-mail des connexions anonymes** et **dossier de téléchargement par défaut** (Options ›
  Général).
- **Modèle des nouveaux sites** (Options › Nouveau site).
- **Réglages propres à un site** remplaçant les options globales : essais et délais,
  maintien de connexion et reconnexion, reprise activée ou non, `FEAT` activé ou non, nom de
  client `CLNT`, liste d'exclusion, priorités, règles « fichier existant », comptes séparés
  pour les envois et les téléchargements.
- **Nom Ident par site** : répondu par le serveur Ident pendant la connexion du site.
- **Plusieurs adresses par site** (fiche du site → Autres adresses) : essayées à tour de
  rôle quand l'une ne répond pas ou refuse la connexion ; celle qui a marché sert en premier
  la fois suivante.
- **Test de disponibilité** d'un site ou de tous les sites d'un dossier (barre d'outils du
  gestionnaire de sites).
- **Connexion fantôme et `-motdepasse`** depuis le gestionnaire de sites (`Maj` / `Ctrl` +
  Connecter) ; `RS_INVERT` est désormais pris en charge dans les scripts.
- **« Happy eyeballs » IPv4/IPv6** : un nom qui a les deux sortes d'adresses se connecte
  par la première qui répond, une route IPv6 morte ne coûte plus un délai complet.
- **FXP entre serveurs IPv6** (`EPSV` côté passif, `EPRT` côté actif).
- **Maintien de connexion** (Options → Maintien de connexion) : commandes anti-inactivité
  envoyées quand une connexion ne sert pas (liste, intervalle, choix au hasard, arrêt
  après un temps d'inactivité, déconnexion).
- **Reconnexion automatique** quand le serveur ferme la connexion (expulsion, inactivité,
  `421`) ou envoie une réponse qui correspond à un masque ; le panneau revient dans son
  dossier. Le moteur de file vérifie une connexion restée inactive avant de la réutiliser.

### Modifié

- La connexion de navigation des panneaux passe de l'interface à `urftp-proto::browse`,
  pour être testée contre le serveur de test.
- Les messages d'erreur de la couche protocole sont désormais tous en anglais (certains
  étaient en français).

### Corrigé

- FTPRush v2 peut être lancé pendant qu'UltraRushFTP tourne : la fenêtre de la liaison mIRC
  s'appelle maintenant `TfmUltraRush` (FTPRush v2 refusait de s'ouvrir tant qu'une fenêtre
  `TfmRush` existait). Notre `rushmirc.dll` trouve les deux applications.
- Une file arrêtée avant la fin était perdue à la fermeture de l'application.
- La barre de connexion rapide suit maintenant les options réseau, de listing et de
  connexion anonyme.

## [0.2.0] - 2026-11-06

Normes de l'interface : navigation au clavier, noms accessibles, contraste WCAG 2.2 AA et
taille des cibles vérifiés par des tests ; tableau de parité avec FTPRush v2.

### Ajouté

- **Tableau de parité** [docs/PARITY.fr.md](docs/PARITY.fr.md) : chaque sujet de l'aide de
  FTPRush v2 (284) avec son état ; la feuille de route est regroupée en blocs G à K à partir
  de ce tableau.
- **Navigation complète au clavier** : `Tab` / `Maj+Tab` parcourent tous les contrôles,
  `Échap` rend les touches à la liste de fichiers ; dans la liste, les flèches,
  `Page préc.` / `Page suiv.`, `Début` / `Fin` déplacent un curseur visible (`Maj` étend la
  sélection, `Ctrl+Espace` sélectionne ou désélectionne une ligne) ; `F6` change de
  panneau ; `Ctrl+T`, `Ctrl+W`, `Ctrl+Tab` et `Ctrl+Maj+Tab` gèrent les onglets.
- Fenêtre **Aide → Raccourcis clavier** (`F1`).
- **Noms accessibles** pour chaque contrôle (lecteurs d'écran) : boutons à symbole, champs
  qui n'avaient qu'un texte indicatif, libellés reliés à leurs champs, lignes de fichiers
  lues avec leur taille et leur date.
- **Tests de l'interface** (`egui_kittest`) qui pilotent la vraie fenêtre au clavier et
  vérifient, en anglais et en français, que chaque contrôle a un nom et que les cibles
  respectent le critère WCAG 2.5.8 (24 px, ou un espacement suffisant).
- **Test de contraste** : chaque couleur de texte et d'indicateur des deux thèmes est
  mesurée par rapport aux rapports WCAG 2.2 AA (4,5:1 pour le texte, 3:1 pour le focus et
  les indicateurs d'état).

### Modifié

- Les couleurs viennent d'une palette par thème ; le thème clair utilise des teintes plus
  foncées pour que les textes du journal, des états et des erreurs restent lisibles, et
  les lignes survolées s'éclaircissent au lieu de s'assombrir.
- Les boutons principaux et destructifs, les champs de texte (24 px de haut) et le contour
  de focus (2 px) ont le même aspect partout.
- Les états ne sont plus indiqués par la couleur seule : symbole d'état de connexion (`●`
  `◌` `✕`) avec une infobulle, `⚠` devant les erreurs, avertissement écrit pour un masque
  invalide.
- Les confirmations (suppression, serveur non approuvé) s'ouvrent avec le clavier sur
  **Annuler** ; `Échap` les annule.
- `F6` remplace `Tab` pour changer de panneau (`Tab` passe désormais d'un contrôle à
  l'autre, comme dans Windows).
- Les fenêtres Options, Gestionnaire de sites et Raccourcis s'ouvrent sous les barres de
  menus et de connexion.

### Corrigé

- Les boutons **Annuler** des boîtes de dialogue de fichiers et de la boîte d'approbation
  étaient toujours en français.
- Les champs de texte des boîtes de dialogue de fichiers reprenaient le focus à chaque
  image : `Tab` ne pouvait pas atteindre les boutons.

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

[Non publié]: https://github.com/werewolf004/UltraRushFTP/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/werewolf004/UltraRushFTP/releases/tag/v0.1.0
