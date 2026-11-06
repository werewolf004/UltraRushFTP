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

## Ensuite

- ⬜ Lecteurs d'écran : annoncer le curseur de la liste de fichiers (les lignes sont nommées
  dans l'arbre AccessKit, mais le curseur ne déplace pas encore le focus système).

## Reste à faire pour la parité avec FTPRush v2

### Connexions et sites
- ⬜ Keep-alive (`NOOP`) et reconnexion automatique en cas d'expulsion.
- ⬜ Liste d'IP par site avec rotation en cas d'échec de login (« Site IP List »).
- ⬜ Ghost login (`!user`) depuis la fiche du site (les scripts gèrent déjà `RS_GHOST`).
- ⬜ Nom Ident par site (enregistré dans la fiche, pas encore répondu par le serveur).
- ⬜ Liste d'exclusion et règles « fichier existant » par site (globales seulement
  aujourd'hui).
- ⬜ FXP entre adresses IPv6 (`EPRT`), connexion IPv4/IPv6 « happy eyeballs ».
- ⬜ Validation sur des serveurs réels (glftpd, vsftpd, ProFTPD, FileZilla Server, IIS,
  OpenSSH).

### File et transferts
- ⬜ Sauvegarde/chargement et planification de la file ; actions de fin de file (veille,
  hibernation, arrêt).
- ⬜ `RS_NOCIRCLE` (le dossier n+1 attend le dossier n), `CompleteFlag`, nombre de
  répétitions et filtres de dossiers/fichiers de `RushApp.FTP.Transfer`.
- ⬜ XDupe, conversion de casse des noms, taille du tampon TCP, mode actif forcé par sens.
- ⬜ Vérifications CRC / SFV / MD5.

### Interface
- ⬜ Journaux FTP / transferts / système séparés ; barre d'état plus complète.
- ⬜ Comparaison de répertoires, recherche rapide et recherche sur les sites, export
  d'arborescence, espace utilisé.
- ⬜ Concepteur de commandes brutes, gestionnaire de raccourcis, actions de souris,
  dispositions enregistrées, couleurs.
- ⬜ Notifications d'événements, icône et verrouillage dans la zone de notification, UPnP.
- ⬜ `InputQuery` interactif, `RushApp.UI.*`, procédures et fonctions dans les scripts.

### Distribution
- ⬜ Manifeste d'application (`longPathAware`, prise en compte du DPI), icône et ressources
  de version.
- ⬜ Signature des binaires.
- ⬜ Installateur (facultatif, en plus du zip portable).

## Plus tard

Les protocoles cloud (WebDAV, S3, Google Drive, OneDrive, Dropbox) sont hors du périmètre
initial.
