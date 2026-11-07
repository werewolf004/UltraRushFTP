# Feuille de route

[English](ROADMAP.md) | **Français**

Légende : ✅ fait · 🟡 en partie · ⬜ prévu. Chaque gros bloc se termine par une version
taguée dont les notes viennent du [CHANGELOG.fr.md](CHANGELOG.fr.md).

## Fait

### Phase 1 — protocoles et moteur (0.1.0)
- ✅ Client FTP écrit pour le projet : contrôle complet des commandes, analyse de `LIST`
  (Unix, DOS/IIS) et `MLSD`, chemins locaux et distants longs, injection CRLF refusée, mot
  de passe masqué dans les journaux.
- ✅ FTPS explicite et implicite, `PBSZ`/`PROT`, reprise de session TLS, politiques de
  certificat (magasin Windows, empreinte approuvée, tout accepter) ; mode actif
  `PORT`/`EPRT`.
- ✅ Proxys SOCKS4/4A/5 et HTTP (contrôle et données), règles de contournement ; serveur
  Ident.
- ✅ FXP : les deux ordres de rôles, `PRET`, reprise, FXP chiffré (`SSCN`/`CPSV`), inversion
  automatique, interruption propre.
- ✅ SFTP : mot de passe ou clé (OpenSSH, PEM, PuTTY), empreintes de clé d'hôte, reprise,
  pipelining, proxy.
- ✅ Moteur de file : multi-transfert par site, limites, dossiers, liste d'exclusion,
  priorités, règles « fichier existant », nouvelles tentatives, arrêt, limitation de
  débit.

### Phase 2 — application (0.1.0)
- 🟡 Interface à onglets : panneaux réorganisables, file, journal, console de script,
  glisser-déposer, raccourcis, dialogues d'approbation, thèmes sombre et clair.
- 🟡 Gestionnaire de sites avec dossiers, fiche complète, mots de passe protégés par DPAPI,
  import depuis FileZilla, FTPRush v1/v2/v3 et FlashFXP.
- 🟡 Fenêtre Options (8 pages) enregistrée automatiquement.

### Phase 3 — automatisation et compatibilité (0.1.0)
- 🟡 Interpréteur Pascal Script et hôte `RushApp.*` (`Login`, `Logout`, `CWD`, `RAW`,
  `Delete`, `Transfer`, `RemoveQueue`, `EnableSite`/`DisableSite`, statistiques…).
- ✅ Liaison mIRC : fenêtre `TfmRush` et `rushmirc.dll` (32/64 bits), testée de bout en
  bout.
- ✅ Ligne de commande : options de démarrage de FTPRush v2 et outil `urftp`.

### Qualité (0.1.0)
- ✅ A — `unsafe` isolé dans `urftp-sys`, aucun `unwrap`/`expect` en production, erreurs
  typées.
- ✅ B — CI/CD sur GitHub Actions (formatage, cargo-deny, Clippy, tests, documentation, DLL
  32/64 bits, versions taguées).
- ✅ C — Code en anglais ; interface et CLI en anglais et en français.
- ✅ D — Documentation en anglais et en français avec captures, références des options et
  des dépendances, licence propriétaire.
- ✅ E — Version 0.1.0 : premier tag, binaires et journal des changements sur GitHub.
- ✅ F — Normes UI/UX (0.2.0) : nom accessible pour chaque contrôle, navigation complète au
  clavier, contraste WCAG 2.2 AA et cibles de 24 px vérifiés par des tests, tests de
  l'interface avec `egui_kittest`, états cohérents (focus, survol, désactivé, erreurs).

## Reste à faire pour la parité avec FTPRush v2

Chaque sujet de l'aide de FTPRush v2, avec son état, est dans
[docs/PARITY.fr.md](docs/PARITY.fr.md) (284 sujets : ✅ 111 · 🟡 32 · ⬜ 124 · ➖ 17).
Le travail prévu est regroupé ci-dessous en blocs, chacun terminé par une version.

### G — Connexions et sites
- ✅ Maintien de connexion (liste de commandes, intervalle, commande aléatoire, arrêt,
  déconnexion) et reconnexion après expulsion ou sur certaines réponses du serveur (G1).
- ✅ Plusieurs hôtes par site avec rotation en cas d'échec de connexion ; test de
  disponibilité des sites (G2).
- ✅ Connexion fantôme (`!user`, `Maj`+connexion) et `-motdepasse` (`Ctrl`+connexion)
  depuis l'interface, `RS_INVERT` dans les scripts (G2).
- ✅ Réglages par site : nom Ident, essais et délais, maintien de connexion, reprise,
  `FEAT`, `CLNT`, liste d'exclusion, priorités, règles « fichier existant », comptes
  séparés pour envois et téléchargements (G3).
- ✅ Réglages de listing accessibles : `LIST -a` (fichiers cachés), `STAT -l`, commande
  personnalisée, espaces de tête, repli `PORT` quand `PASV` échoue, fuseau horaire, jeu de
  caractères des serveurs non UTF-8 (G4).
- ✅ Réglages du mode actif accessibles : adresse externe, plage de ports locaux, adresse
  IP locale (G4).
- ✅ E-mail anonyme, dossier de téléchargement par défaut, réglages par défaut des sites (G4).
- ✅ Proxys nommés, `NOOP` pendant les transferts, listings et transferts chiffrés
  séparément, Ident seulement pendant la connexion ; plusieurs clés SSH par site, rôles FXP
  par site ; description du site, format du listing, adresse web et « Copier l'adresse
  HTTP » (G5).
- ✅ FXP entre adresses IPv6 (`EPSV` / `EPRT`), connexion IPv4/IPv6 « happy eyeballs » (G2).
- 🟡 Validation sur des serveurs réels (glftpd, vsftpd, ProFTPD, FileZilla Server, IIS,
  OpenSSH) : script et liste de contrôle prêts ([docs/VALIDATION.fr.md](docs/VALIDATION.fr.md)),
  résultats à recueillir (G6).

### H — File et transferts
- ⬜ Pause, modification d'un élément (source, cible, mode, inversion FXP), marquer en
  échec, retirer les échecs, informations sur la file.
- ✅ File non terminée enregistrée automatiquement et proposée au démarrage.
- ⬜ Charger / enregistrer un fichier de file à la main.
- ⬜ Options d'essais : par sens, essai cyclique, relance des échecs en fin de file,
  réinitialisation au démarrage.
- ⬜ Fin de file : rester connecté / déconnecter, veille, veille prolongée, arrêt, quitter
  (avec confirmation).
- ⬜ `RushApp.FTP.Transfer` : `RS_NOCIRCLE`, `CompleteFlag`, `RepeatCount`, `Depth`,
  `SubCount`, tri, filtres dossiers/fichiers, `%F%` / `%D%`.
- ⬜ Transfert conditionnel et synchronisation (masques, dates, tailles, suppression des
  absents).
- ⬜ XDupe, conversion de casse des noms, mode ASCII et liste ASCII, mode actif forcé,
  tampon TCP, `MODE Z`, retour en arrière à la reprise, garde d'espace disque, pas de
  limite de vitesse pour les adresses locales.
- ⬜ CRC : vérification SFV au téléchargement, `XCRC`, réajout des fichiers défectueux,
  création de SFV / MD5 ; règles SFV.
- ⬜ Règles de dossiers : retirer les dossiers vides laissés par un transfert, garder la
  date du serveur, entrer dans un dossier créé, refuser un dossier du même nom que son
  parent, retirer la cible de 0 octet quand la source FXP refuse.

### I — Navigation et opérations sur les fichiers
- ⬜ Filtre rapide (`Ctrl+F`), marques, sélection par masque, copie de plusieurs noms,
  renommage multiple.
- ⬜ CHMOD, déplacer vers, modifier la date, listing brut, colonnes attributs /
  propriétaire / groupe.
- ⬜ Comparer des dossiers (nom, taille, partiel, masquer les identiques), chercher sur les
  sites, espace utilisé, export d'arborescence.
- ⬜ Cache des dossiers ; voir et modifier les fichiers distants avec des programmes
  externes.
- ⬜ Glisser depuis l'Explorateur, coller des fichiers locaux, navigation des deux côtés
  (`Maj`+signet, `Maj`+double-clic).

### J — Interface
- ⬜ Journaux FTP / transferts / système séparés, journal sur disque, noms des sites dans
  le journal ; barre d'état avec durées de la file et texte de progression.
- ⬜ Boîte RAW (plusieurs lignes, tous les sites), commandes de site prédéfinies,
  concepteur de commandes.
- ⬜ Menu des favoris (sites, sites récents), ajout d'une connexion rapide au gestionnaire
  de sites, téléchargement manuel et URL incrémentées, surveillance du presse-papiers.
- ⬜ Gestionnaire de raccourcis, actions de souris réglables, confirmations (quitter,
  fermer un onglet occupé).
- ⬜ Icône de notification, verrouillage, notifications (sons, bulles, surveillance de
  dossiers), clignotement de la fenêtre.
- ⬜ Surlignage, polices, couleurs personnalisées (avec le test de contraste), options de
  tri, options d'onglets, dispositions enregistrées.
- ⬜ Gestionnaire de sites : mode liste, import texte brut et UltraFXP, export texte / CSV
  / HTML / XML (sans mots de passe), mot de passe maître facultatif ; sauvegarde et
  restauration de la configuration, instance unique ; fenêtre À propos.

### K — Scripts
- ⬜ Tableaux, index, `case`, procédures et fonctions de l'utilisateur.
- ⬜ `RushApp.UI.Selection`, `RushApp.UI.Bar` / `Menu`, `OnSwitchLayout`.
- ⬜ `InputQuery` interactif, `MatchExpr`, `MatchString` ; `ShellExecuteCommand` derrière
  une règle de confirmation.
- ⬜ Lecteurs d'écran : annoncer le curseur de la liste de fichiers (les lignes sont nommées
  dans l'arbre AccessKit, mais le curseur ne déplace pas encore le focus système).

### Distribution
- ⬜ Manifeste d'application (`longPathAware`, prise en compte du DPI), icône et ressources
  de version.
- ⬜ Signature des binaires.
- ⬜ Installateur (facultatif, en plus du zip portable).
- ⬜ Redirection de ports UPnP (mode actif, Ident).

## Plus tard

Les protocoles cloud (WebDAV, S3, Google Drive, OneDrive, Dropbox) sont hors du périmètre
initial.
