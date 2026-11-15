# Journal des changements

[English](CHANGELOG.md) | **Français**

Les changements notables d'UltraRushFTP. Le format suit
[Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) et le projet utilise le
[versionnage sémantique](https://semver.org/lang/fr/). Les notes des versions publiées sur
GitHub reprennent la version anglaise ([CHANGELOG.md](CHANGELOG.md)) ; ce fichier en est
la traduction.

## [Non publié]

### Ajouté

- Option de site *Utiliser AUTH SSL au lieu d'AUTH TLS* (FTPS explicite), pour les
  serveurs qui raccrochent sur `AUTH TLS` (mode *AUTH SSL* de FTPRush).

### Modifié

- La signature du code n'est pas prévue : les versions restent non signées (vérifier les
  fichiers `.sha256`).
- L'import UltraFXP est abandonné : le programme n'est plus utilisé depuis longtemps.

### Corrigé

- FTPS : les serveurs au certificat ECDSA P-521 (glftpd et OpenSSL récents)
  n'interrompent plus la négociation TLS (`HandshakeFailure`), et les certificats à clé
  RSA de 1024 bits (anciens certificats auto-signés) sont acceptés.
- SFTP : les serveurs plus anciens (vieux OpenSSH, NAS, routeurs, serveurs Windows sans
  curve25519) se connectent : échanges de clés ECDH NIST et Diffie-Hellman SHA-1,
  chiffrements AES-CBC et 3DES, HMAC-SHA1 et clés d'hôte DSA sont proposés en dernier,
  après les algorithmes sûrs.

## [0.13.0] - 2026-11-14

Transferts avancés (bloc O) et finitions (bloc P).

### Ajouté

- `MODE Z` aussi pour les **listes de dossiers**, et le choix de ce qui est compressé :
  listes, réceptions, envois (Options › Transfert avancé).
- **Dossier d'état SFV** à côté des téléchargements vérifiés, tenu à jour :
  `[UltraRushFTP] - ( 7 of 10 files OK = 70% ) - INCOMPLETE` (option).
- Scripts : `RushApp.FTP.Transfer` avec **`RS_CHECKLOGIN`** se connecte d'abord à ses sites
  (en même temps, à côté de l'interface) et laisse de côté ceux qui refusent la connexion.
- **Fenêtre d'aide** (`F1`, menu Aide) : la documentation de référence intégrée (en anglais
  ou en français), avec un index de ses sections, une recherche et les raccourcis clavier.
- Console de script : **Ouvrir…**, **Enregistrer**, **Enregistrer sous…** de fichiers
  `.pas` ; le nom du fichier porte `•` tant que le texte a changé.
- Options › Transferts : **arrêter et reprendre sans données pendant N secondes** (FTP et
  SFTP) : un transfert bloqué s'arrête avant le délai de réponse et reprend par les
  tentatives.

### Corrigé

- Menu du clic droit des fichiers et de la file d'attente : **Tout sélectionner**.
- Gestionnaire de sites : les sites et les dossiers se déplacent dans l'arbre par
  **glisser-déposer** (sur un dossier, ou avant un autre site).
- Après un transfert compressé, une liste sur la même connexion était lue compressée :
  elle repasse en `MODE S` (ou lit la liste compressée).

### Modifié

- Le dépôt est public. La CI ne se lance pas pour les changements de la documentation
  seule ; Dependabot laisse de côté les versions majeures et les nouvelles versions
  d'egui (migrées à la main).
- parking_lot 0.13, flate2 1.1.11.

## [0.12.0] - 2026-11-12

Apparence (bloc N).

### Ajouté

- Options › Interface : **polices** de l'interface, des listes de fichiers et des journaux
  (les polices TrueType / OpenType de Windows, par leur nom) ; **onglets sur plusieurs
  lignes** et **largeur minimale d'un onglet** ; **grands boutons** dans les barres
  d'outils ; **liens listés à part**, entre les dossiers et les fichiers (🔗) ; **couleur
  propre des barres de progression**, par thème, avec son contraste ; **bouton Favoris
  scindé** dont la partie principale se connecte au dernier site.
- **Actions du double-clic** : sur une ligne de journal (la copier), sur un site du
  gestionnaire (se connecter), sur le titre d'un panneau de fichiers (actualiser, dossier
  parent, basculer local / distant), sur un onglet (nouvel onglet des mêmes sites, fermer)
  et sur un élément de la file (modifier, réessayer, retirer).
- Favoris : les **connexions rapides** sont gardées (dix, jamais leur mot de passe) ; l'une
  d'elles se reconnecte aussitôt si elle est anonyme, sinon le champ mot de passe attend
  le mot de passe.
- Affichage › Dispositions › **Panneaux en onglets** : les deux panneaux de fichiers dans
  une seule zone, leurs titres basculent de l'un à l'autre.
- Options › Notifications : **faire clignoter le bouton de la barre des tâches pendant
  les transferts** en arrière-plan.

### Corrigé

- Un clic sur le nom d'un fichier ou d'un élément de la file le sélectionne (le texte
  prenait le clic) : le double-clic sur un nom le transfère ou l'ouvre de nouveau.
- Gestionnaire de sites : la barre d'outils garde sa taille quand un site est choisi,
  l'arbre ne bouge plus sous le pointeur.
- Les titres des panneaux de fichiers ont un nom pour les lecteurs d'écran (« Panneau
  gauche : Local (actif) »).

## [0.11.0] - 2026-11-12

Confort au quotidien (bloc M).

### Ajouté

- **Bascules ⬇ / ⬆ de limite de débit** dans la barre d'état : chaque limite activée ou
  non en un clic, transferts en cours compris.
- Options › Interface : **tailles des fichiers** en octets, Ko ou dynamique ; **fichiers
  locaux cachés** affichés ou non ; **colonnes ajustées à leur contenu** (local, distant,
  file) ; le dossier local d'un téléchargement en cours **relu toutes les N secondes** ;
  **clic droit : menu de l'Explorateur Windows** dans les panneaux locaux (aussi *Menu de
  l'Explorateur Windows…* dans le menu du panneau).
- Liste d'exclusion : **s'applique au** contenu des dossiers (auto), aux éléments choisis
  (manuel) ou aux deux ; les éléments qu'elle exclurait sont **affichés en couleur** et
  barrés.
- **Jetons de date** dans les signets : `%yyyy%`, `%yy%`, `%mm%`, `%m%`, `%dd%`, `%d%`,
  `%ww%`, `%w%` (semaine ISO).
- FTP › **Traduction d'adresse IP…** : adresse hexadécimale ou numérique vers la forme
  pointée.

## [0.10.0] - 2026-11-12

Fenêtres, onglets et tâches (bloc L).

### Ajouté

- **Fenêtre des tâches** (menu Affichage) : files et Pascal Scripts lancés une fois, toutes
  les N minutes ou certains jours de la semaine à une heure ; **Planifier la file en
  tâche…** dans le menu ⋯ de la file (la file se lance dans un nouvel onglet, qui peut se
  fermer seul à la fin) ; lancer tout de suite, activer / désactiver ; les heures manquées
  pendant que l'application était fermée sont signalées, pas rattrapées (`tasks.json`).
- **Liste des fenêtres** (menu Affichage) : une ligne par onglet avec ses panneaux, ses
  connexions (FTP) et ce que fait sa file (Action) ; un clic affiche l'onglet.
- **Nouvel onglet à partir de l'actuel** (menu Fichier, `Ctrl+Maj+T`, clic droit sur un
  onglet) : même disposition, mêmes dossiers locaux et mêmes connexions.
- Options › Général › **Les deux panneaux pour des sites** : une deuxième connexion va dans
  le panneau gauche avant d'ouvrir un nouvel onglet.

## [0.9.0] - 2026-11-11

Distribution.

### Ajouté

- **Installateur** `UltraRushFTP-<version>-windows-x64-setup.exe` (Inno Setup) à côté du
  zip portable : pour l'utilisateur courant sans droits d'administrateur, ou pour tous les
  utilisateurs ; en anglais ou en français ; raccourci du menu Démarrer, raccourci du
  bureau au choix, DLL mIRC et documentation ; le désinstallateur garde les réglages de
  `%APPDATA%\UltraRushFTP`.
- **Icône** de l'application (fenêtre, barre des tâches, zone de notification, fichiers),
  **informations de version** dans les propriétés de `UltraRushFTP.exe`, `urftp.exe` et
  `rushmirc.dll`, et **manifeste d'application** : chemins longs (`longPathAware`), DPI par
  écran, Windows 10/11, style visuel actuel des boîtes de dialogue du système.
- **UPnP** (Options › Connexion) : le routeur ouvre chaque port du mode actif le temps de
  son usage et le port 113 vers le serveur Ident, et son adresse publique est annoncée par
  `PORT` ; **Relancer le service UPnP** dans le menu FTP et sur la page d'options. Écrit
  pour le projet (SSDP et SOAP), sans nouvelle dépendance à l'exécution.
- Signature prête : les versions publiées signent les binaires et l'installateur dès qu'un
  certificat est enregistré dans les secrets du dépôt (`scripts/sign.ps1`) ; non signées
  d'ici là.

### Modifié

- Les paquets des versions et de la CI sont produits par `scripts/package.ps1` (zip,
  installateur, sommes de contrôle).

### Corrigé

- Barre d'outils du gestionnaire de sites : quand la fenêtre est étroite, **Connecter** et
  le nombre de sites passent sur une ligne à part au lieu de recouvrir **Exporter** et le
  bouton **Liste**.

## [0.8.0] - 2026-11-11

Propriétés des sites aussi complètes que celles de FTPRush v2.

### Ajouté

- **Fiche de site** : UTF-8 forcé, identifiant envoyé en UTF-8, serveur derrière un NAT
  (connexions de données vers l'adresse du serveur), `PBSZ 0` désactivable, FXP chiffré
  automatique / jamais / toujours, fuseau horaire du serveur, reprise Détection auto / Oui
  / Non, les fonctions utilisées quand elles sont annoncées (`PRET`, UTF-8, `MODE Z`,
  `XCRC`, `MLSD`, `SSCN`, `CPSV`), transferts du site (casse des noms à l'envoi, à la
  réception et en FXP, XDUPE, NOOP pendant les transferts, tampons d'envoi et de
  réception), une adresse et un port pour les comptes séparés, le nom du site dans les
  journaux, les dossiers surveillés du site, et les **statistiques du serveur** (dernier
  accès, connexions, envoyé, reçu, FXP depuis et vers lui ; copie et remise à zéro des
  totaux).
- **Règles « fichier existant » par sens** (envoi, réception, FXP), dans les options et par
  site ; les règles des versions précédentes s'appliquent aux trois.
- Casse des noms en FXP et *Relire le dossier quand XDUPE ne renvoie rien* (Transfert
  avancé).
- **Exporter… / Importer…** de la liste d'exclusion (JSON, ou un masque par ligne).
- Tampons d'envoi et de réception réglés à part dans Transfert avancé (le tampon unique
  des versions précédentes remplit les deux).

### Modifié

- Un onglet connecté à un site du gestionnaire porte le nom du site au lieu de son
  adresse.

## [0.7.0] - 2026-11-08

Scripts (bloc K).

### Ajouté

- **Pascal Script** : tableaux (`[2, 4, 6]`, imbriqués, `M[0, 2]`, `in`, `SetLength`,
  `Low`, `High`), index de chaînes (`S[1] := 'A'`), `case … of` (étiquettes de tout type,
  intervalles), `break`, `continue`, `const`, `var` sans type, **procédures et
  fonctions** avec paramètres `var`, variables locales, `Result` et récursivité ; `Chr`,
  `Ord`, `Abs`, `Odd`.
- `RushApp.UI.Selection` (`Prepare`, `Count`, `Items[i]` avec `Name`, `Size`, `Date`,
  `Attr`, `Owner`, `Group`, `IsFolder`, `FullPath`, `IsRemoteSide`, `IsFTPConnected`,
  `FTPName`) et l'événement `RushApp.UI.OnSwitchLayout` (chargement d'une disposition
  enregistrée).
- `MatchExpr` (expressions régulières) et `MatchString` (jokers).
- `ShellExecuteCommand` : demandé à chaque fois par défaut, avec le programme, ses
  paramètres et l'origine du script ; Options › Sécurité permet de lancer les programmes
  sans demander, jamais, ou de garder une liste de programmes toujours autorisés.
- Les lecteurs d'écran entendent la ligne sous le curseur de la liste de fichiers (zone
  live).

### Modifié

- Les scripts (console, `RushScript` / `RushCommand` de mIRC, `-s`, `URUSHFTP_SCRIPT`)
  tournent dans un fil à eux : la fenêtre reste utilisable, `InputQuery` ouvre une boîte
  de saisie et l'attend, et la console a un bouton **Arrêter**. `Invoke` répond toujours
  aussitôt.
- `RushApp.UI.Bar` et `RushApp.UI.Menu` répondent par une erreur claire : l'aide de FTPRush
  n'en décrit aucun membre.

## [0.6.0] - 2026-11-08

Interface (bloc J).

### Ajouté

- **Journaux** : journaux FTP, transferts et système dans chaque onglet, avec l'heure et
  le nom des sites, un nombre de lignes propre, et des fichiers quotidiens sur disque
  (option) ; la barre d'état montre la progression de la file avec un texte au choix
  (`%p`, `%b`, `%s`, `%e`, `%c`).
- **Commandes RAW** (`Ctrl+R`) : plusieurs lignes, vers le panneau actif, les deux
  panneaux ou tous les sites connectés, réponses listées ; **commandes de site**
  prédéfinies dans le menu FTP, modifiées dans les options (`%1` demande un paramètre).
- **Favoris** dans la barre d'outils (sites récents, tous les sites), `➕` enregistre une
  connexion rapide comme site ; **Téléchargement manuel** d'adresses avec listes
  incrémentées (`[01-12]`) ; la surveillance du presse-papiers l'ouvre (ou met en file
  aussitôt).
- **Gestionnaire de raccourcis** (Options › Raccourcis, `F1` montre les touches en
  vigueur) ; double-clic sur un fichier : transférer, afficher ou rien ;
  **confirmations** avant de quitter avec des éléments restants, de fermer un onglet
  occupé, de retirer des éléments de la file.
- **Notifications** : icône dans la zone de notification (réduire dedans), sons et bulles
  des événements (connecté, déconnecté, file terminée, échec d'un transfert,
  nouveautés), bouton de la barre des tâches qui clignote à la fin d'une file,
  **dossiers surveillés**.
- **Affichage** : taille de l'interface et des journaux, noms surlignés et couleurs des
  journaux au choix avec leur contraste affiché, dossiers d'abord ou non, plus grand /
  plus récent d'abord, onglets sous les panneaux, **dispositions enregistrées**
  (Affichage → Dispositions, disposition des nouveaux onglets).
- **Gestionnaire de sites** : mode liste, import d'un fichier texte d'adresses, export en
  texte, CSV, HTML ou XML FTPRush v2 (jamais les mots de passe).
- **Sécurité** : mot de passe maître facultatif (gestionnaire de sites, options,
  *Fichier → Verrouiller*), seule une empreinte PBKDF2 salée est gardée ; **sauvegardes**
  de la configuration (maintenant, ou chaque jour, 4 gardées) et restauration ;
  **instance unique**.

### Corrigé

- Le test de disponibilité d'un site échouait de temps en temps : son résultat est
  dessiné avant d'être vérifié.

## [0.5.0] - 2026-11-08

Navigation et opérations sur les fichiers (bloc I).

### Ajouté

- **Panneaux** : filtre rapide (`Ctrl+F` : mots, `-mot`, masques, expression régulière ;
  `F5` réaffiche tout), marques (`Ctrl+M`, en gras souligné ; tout démarquer, sélectionner
  les marqués, copier leurs chemins), sélection / désélection par masque (`+` / `-`),
  copie des noms (`Ctrl+Maj+C`), *Renommer plusieurs…* (modèle avec `%f` `%n` `%e` `%c`,
  remplacement par texte ou expression régulière, casse, aperçu), *Déplacer vers…* ;
  colonnes droits / propriétaire / groupe dans les panneaux distants, attributs Windows
  dans les locaux ; nouveaux menus **Édition** et **FTP**.
- **Opérations distantes** : *Droits (CHMOD)* (`SITE CHMOD`, `SETSTAT`, contenu des
  dossiers aussi), *Modifier la date* (`MFMT`, sinon `SITE UTIME` ; `SETSTAT`), *Voir le
  listing brut*.
- **Outils de dossier** : *Comparer les panneaux* (nom ou nom et taille, extensions
  ignorées, masquer les identiques, sélectionner les autres), *Espace utilisé*, *Exporter
  l'arborescence* (retrait, tailles, dossiers seulement), *Chercher sur les sites* (mots et
  masques, exclusions, type, dates, tailles, profondeur ; résultats vers la file ou
  copiés en adresses `ftp://`). Parcours et recherches ont leur propre connexion ; les
  dossiers locaux se parcourent aussi.
- **Cache des dossiers** (Options › Navigation, ou par site) : les dossiers revisités sont
  montrés de mémoire, oubliés quand un transfert y écrit ; *Vider le cache de ce site /
  tous les caches*.
- **Voir et modifier les fichiers distants** (`F3` / `F4`, double-clic pour voir) :
  programmes choisis par masques dans Options › Navigation (sinon le Bloc-notes, jamais le
  programme du type de fichier) ; un fichier modifié est renvoyé à chaque enregistrement ;
  fenêtre *Fichiers en cours de modification*.
- **Explorateur** : les fichiers glissés sur un panneau distant ou collés par `Ctrl+V`
  sont envoyés dans son dossier.
- **Deux panneaux** : `Maj`+double-clic ouvre un dossier des deux côtés ; le nouveau menu
  *Signets* d'un panneau distant va à un signet, avec `Maj` aussi dans son dossier local.

### Corrigé

- Supprimer un dossier distant laissait la connexion de navigation dans le dossier
  supprimé : le panneau revient maintenant dans son propre dossier après chaque série
  d'opérations.

## [0.4.0] - 2026-11-08

File et transferts (bloc H).

### Ajouté

- **File** : pause et reprise (les transferts interrompus reprennent depuis leur fichier
  partiel), sélection de lignes (`Ctrl`, `Maj`), *Modifier…* (source, cible, mode),
  *Inverser le FXP*, *Marquer en échec*, *Retirer les échecs*, *Vider la file*,
  *Informations sur la file* (nombres, tailles, temps restant), fichiers de file `.urq`
  chargés et enregistrés à la main.
- **Nouvelles tentatives** : par sens, relance cyclique, tours de relance une fois la file
  terminée, relance des échecs quand une file démarre automatiquement.
- **Fin de la file** : déconnecter, quitter, veille, veille prolongée ou arrêt, avec un
  compte à rebours annulable ; déconnecter un site retire ses éléments en attente (option) ;
  source et cible de chaque élément dans le journal (option).
- **`RushApp.FTP.Transfer`** : tous les paramètres — filtres de dossiers et de fichiers
  avec `%F%` / `%D%`, `RS_EXPR`, `RS_NOSKIP`, `CompleteFlag` et `RepeatCount` (dossier
  relu), `RetryCount`, `FolderSort` / `FileSort`, `Depth`, `SubCount`, `RS_NOCIRCLE`,
  `RS_APPEND` (sinon un onglet par paire), `RS_LOGOUT`, `RS_DELEMPTYTABS`.
- **Transfert conditionnel** (clic droit dans un panneau) : masques de noms, dates,
  tailles, liste d'exclusion ignorée, sous-dossiers, déconnexion à la fin ;
  **synchronisation** (comparaison des tailles et des dates, suppression de ce qui manque à
  la source, fichiers existants seulement, dans les deux sens) ; schémas enregistrés.
- Options **Transfert avancé** : mode de transfert et liste ASCII (pas de reprise pour les
  fichiers ASCII), casse des noms cibles, mode actif pour tous les sites, `XDUPE`,
  `MODE Z` avec exceptions, tampon TCP, recul avant reprise, espace disque gardé libre,
  pas de limite de vitesse sur le réseau local.
- **CRC** : vérification SFV des téléchargements (aussi quand le SFV arrive en dernier),
  `XCRC` après les envois et le FXP, fichiers défectueux supprimés et retransférés, un seul
  SFV par dossier ; *Créer un fichier SFV / MD5* dans le panneau local.
- **Règles de dossiers** : dossiers cibles vides supprimés, date du serveur gardée,
  nouveau dossier ouvert, sous-dossier homonyme du parent refusé, cible de 0 octet d'un FXP
  refusé supprimée.

### Modifié

- Les captures de la documentation sont dessinées hors écran : aucune fenêtre ne s'ouvre
  et rien ne prend le focus pendant leur création. Elles ne montrent plus la barre de titre
  de Windows.

### Corrigé

- **Tout arrêter** termine la file : les éléments en attente démarraient dès que ceux en
  cours s'étaient arrêtés.

## [0.3.0] - 2026-11-07

Connexions et sites (bloc G) : maintien de connexion et reconnexion, plusieurs hôtes par
site, réglages par site, réglages des listes et du mode actif, proxys nommés, rôles FXP,
file non terminée gardée d'un lancement à l'autre, barre de connexion sur chaque panneau,
script de validation ; vérifié sur glftpd.

### Ajouté

- **Barre de connexion sur chaque panneau de fichiers**, comme FTPRush : *Connexion*
  (sites du gestionnaire, dossiers en sous-menus, `Maj` connexion fantôme, `Ctrl`
  `-password`), *Reconnecter*, *Déconnecter*, et une **bascule Local ⇄ site** qui garde la
  connexion ouverte derrière le dossier local. Champ du chemin modifiable dans les
  panneaux distants.
- **Menu Connexion** dans la barre de menus, qui agit sur le panneau actif.
- **Aide → À propos d'UltraRushFTP** : version, compilation et licence.
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

[Non publié]: https://github.com/werewolf004/UltraRushFTP/compare/v0.13.0...HEAD
[0.13.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/werewolf004/UltraRushFTP/releases/tag/v0.1.0
