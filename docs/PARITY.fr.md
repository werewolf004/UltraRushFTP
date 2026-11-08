# Parité avec FTPRush v2

[English](PARITY.md) | **Français**

Chaque sujet de l'aide de FTPRush v2 (235 pages, lues en entier le 2026-11-06) et
l'état d'UltraRushFTP. L'aide sert uniquement de référence fonctionnelle : rien n'en est
copié et FTPRush lui-même n'est pas décompilé.

Légende : ✅ fait · 🟡 en partie · ⬜ prévu · ➖ hors périmètre (raison donnée).

Résumé (287 sujets) : ✅ 228 · 🟡 25 · ⬜ 16 · ➖ 18.

## Fenêtre principale et menus

### Fichier

| Sujet | État | Remarques |
|---|---|---|
| Nouvelle fenêtre | ✅ | Nouvel onglet (`+`, `Ctrl+T`) |
| Nouvelle fenêtre à partir de l'actuelle | ⬜ | Dupliquer un onglet avec ses connexions |
| Fermer la fenêtre courante | ✅ | `✕`, `Ctrl+W` (déconnecte) |
| Réduire dans la zone de notification | ✅ | Options › Notifications ; un clic sur l'icône ramène la fenêtre |
| Verrouiller dans la zone de notification | ✅ | Fichier › Verrouiller, avec le mot de passe maître (Options › Sécurité) |
| Sauvegarder / restaurer la configuration | ✅ | Fichier › Sauvegarder / Restaurer : les fichiers du dossier de données dans `backup/<date>` |
| Quitter | ✅ | |

### Édition et marquage

| Sujet | État | Remarques |
|---|---|---|
| Recherche (filtre rapide) | ✅ | `Ctrl+F` : mots, `-mot`, masques, expression régulière ; `F5` réaffiche tout |
| Renommer | ✅ | `F2` |
| Supprimer | ✅ | `Suppr`, avec confirmation |
| Créer un dossier | ✅ | `F7` |
| Renommer plusieurs fichiers | ✅ | Modèle (`%n`, `%e`, `%c`), remplacement (expression régulière), casse, aperçu |
| Copier les noms sélectionnés | ✅ | `Ctrl+Maj+C`, un nom par ligne |
| Marquer / démarquer, tout démarquer, sélectionner les marqués, exporter les marques | ✅ | `Ctrl+M` ; lignes marquées en gras soulignées |
| Sélectionner (par masque) | ✅ | `+` / `-`, masques séparés par `;` |
| Tout sélectionner | ✅ | `Ctrl+A` |

### Affichage

| Sujet | État | Remarques |
|---|---|---|
| Actualiser | ✅ | `F5` |
| Liste des fenêtres | 🟡 | Barre d'onglets seulement ; pas de liste avec colonnes connexion et action |
| Fenêtre de file d'attente | ✅ | Panneau du dock |
| Journal système, journal des transferts, journal FTP | ✅ | Journaux FTP, transferts et système de chaque onglet, avec l'heure |
| Fenêtre des tâches | ⬜ | Tâches planifiées |
| Colonnes distantes taille / date | ✅ | |
| Colonnes distantes attributs / utilisateur / groupe | ✅ | Panneaux distants ; les panneaux locaux montrent les attributs Windows |
| Colonnes locales taille / date | ✅ | |
| Colonnes de la liste des fenêtres | ⬜ | |
| Lignes de grille | ➖ | Lignes alternées à la place |
| Ajustement automatique des colonnes | 🟡 | Colonnes redimensionnables, sans ajustement automatique |
| Fichiers en cours d'édition | ✅ | Menu FTP → Fichiers en cours de modification |

### FTP

| Sujet | État | Remarques |
|---|---|---|
| Boîte de connexion rapide | ✅ | Barre de connexion rapide |
| Attributs (CHMOD) | ✅ | `SITE CHMOD` (FTP), `SETSTAT` (SFTP), contenu des dossiers aussi |
| Déplacer vers | ✅ | Distant (`RNFR`/`RNTO`) et local |
| Modifier la date de fichiers FTP | ✅ | `MFMT`, sinon `SITE UTIME` ; `SETSTAT` en SFTP |
| Calculer l'espace FTP utilisé | ✅ | Distant et local, avec une profondeur |
| Exporter l'arborescence | ✅ | Retrait, tailles, dossiers seulement ; copie ou enregistrement |
| Chercher des fichiers sur les sites | ✅ | Mots et masques, exclusions, type, dates, tailles, profondeur ; vers la file, copie de l'adresse |
| Vider le cache du site / de tous les sites | ✅ | |

### Outils → File d'attente

| Sujet | État | Remarques |
|---|---|---|
| Démarrer / mettre en pause la file | ✅ | La pause interrompt les transferts, qui reprennent depuis leur fichier partiel |
| Ajouter la sélection à la file | ✅ | Bouton Transférer, `Espace`, glisser-déposer |
| Transfert conditionnel | ✅ | Masques, dates, tailles, synchronisation, schémas enregistrés |
| Vider la file courante | ✅ | ⋯ File → Vider la file (sauf en cours) |
| Monter / descendre / tout en haut / tout en bas | ✅ | Clic droit |
| Modifier un élément de la file | ✅ | Source, cible, mode ; inverser le FXP |
| Relancer tous les échecs | ✅ | « Relancer les échecs » |
| Relancer les échecs sélectionnés | ✅ | Clic droit → Relancer |
| Retirer tous les échecs | ✅ | |
| Marquer la sélection / tout en échec | ✅ | Clic droit sur la sélection, ou ⋯ File |
| Informations sur la file | ✅ | Nombres, tailles, temps restant |
| Restaurer la file, charger / enregistrer un fichier de file | ✅ | File non terminée au démarrage ; fichiers `.urq` à la main |
| Planifier la file en tâche | ⬜ | |
| Bascules de limite de vitesse d'envoi / de réception | 🟡 | Limites dans les options ; pas de bascule en un clic |

### Outils → autres

| Sujet | État | Remarques |
|---|---|---|
| Gestionnaire de sites | ✅ | |
| Ajouter au gestionnaire de sites | ✅ | `➕` à côté de Connexion dans la barre rapide |
| Commandes (commandes de site prédéfinies) | ✅ | FTP › Commandes de site ; `%1` demande un paramètre |
| Concepteur de commandes | ✅ | Options › Interface › Commandes de site |
| Envoyer une commande RAW | ✅ | FTP › Envoyer une commande RAW (`Ctrl+R`) |
| Comparer le contenu des dossiers (nom, taille, partiel, masquer les identiques) | ✅ | Menu FTP → Comparer les panneaux |
| Voir le listing brut | ✅ | FTP : tel qu'envoyé par le serveur ; SFTP : lignes `ls -l` |
| Concepteur de scripts | 🟡 | Console de script dans chaque onglet ; pas d'éditeur avec fichiers |
| Langues | 🟡 | Anglais et français |
| Gestionnaire de raccourcis | ✅ | Options › Raccourcis |
| Téléchargement manuel depuis un site, liste d'URL incrémentées | ✅ | Fichier › Téléchargement manuel ; listes `[1-12]` / `[01-12]` |
| Surveiller le presse-papiers, transfert automatique | ✅ | Options › Interface › Presse-papiers |
| Mode de transfert auto / binaire / ASCII | ✅ | Options › Transfert avancé, et par élément (Modifier) |
| À la fin des transferts : déconnecter / rester connecté | ✅ | Options › Transferts |
| À la fin des transferts : éteindre / veille / veille prolongée | ✅ | Avec un compte à rebours annulable |
| À la fin des transferts : quitter | ✅ | Options › Transferts, et `-rx` |
| Créer un fichier SFV ou MD5 | ✅ | Clic droit dans un panneau local |
| Traduction d'adresse IP | ⬜ | Adresse hexadécimale / numérique vers la forme pointée |
| Redémarrer le service UPnP | ⬜ | Vient avec UPnP |
| Options | ✅ | Fenêtre Options |

### Aide

| Sujet | État | Remarques |
|---|---|---|
| Index de l'aide | 🟡 | `F1` liste les raccourcis ; documentation complète dans `docs/` |
| Assistant FTPRush | ➖ | Assistant de personnalisation propre à la boîte à outils Delphi |
| Vérifier les nouvelles versions | ➖ | Pas de mise à jour en ligne dans cette phase |
| À propos | ✅ | Aide › À propos |

### Interface principale et prise en main

| Sujet | État | Remarques |
|---|---|---|
| Disposition principale (panneaux, journaux, file, barre d'état) | 🟡 | Dock avec panneaux, file, trois journaux, script ; dispositions enregistrées ; pas de fenêtre des tâches |
| Indicateur de côté | 🟡 | `▸` sur le panneau actif |
| Se connecter depuis le menu des favoris | ✅ | `★ Favoris ▾` dans la barre d'outils |
| Se déconnecter d'un site | ✅ | |
| Verrouiller / déverrouiller barre d'outils et dock | ➖ | Le dock se réorganise par glisser ; pas de barre d'outils à verrouiller |
| Connexion rapide à un site | ✅ | `Tab` entre les champs, `Entrée` connecte |
| Personnaliser l'interface (styles) | ➖ | Styles de la boîte à outils Delphi (XP, Office 2003…) ; thèmes sombre et clair à la place |

## Boîtes de dialogue

| Sujet | État | Remarques |
|---|---|---|
| Ajouter ou modifier une règle de la liste d'exclusion | ✅ | Modifiée dans la grille des options |
| Signets | 🟡 | Nom, chemins distant et local ; jetons de date (`%yyyy%`, `%mm%`…) pas encore |
| Créer des sommes de contrôle CRC | ✅ | Fichiers SFV et MD5 |
| Transfert conditionnel | ✅ | Voir *Transfert conditionnel* |
| Calculer l'espace FTP utilisé | ✅ | |
| Modifier un élément de la file | ✅ | |
| Exporter l'arborescence | ✅ | |
| Recherche rapide | ✅ | Voir *Recherche (filtre rapide)* |
| Chercher des fichiers sur les sites | ✅ | |
| Gestionnaire de raccourcis | ✅ | Voir *Gestionnaire de raccourcis* |
| Boîte RAW (`Ctrl+R`, plusieurs lignes, tous les sites) | ✅ | |
| Téléchargement manuel | ✅ | |
| URL incrémentées | ✅ | |

## Gestionnaire de sites

| Sujet | État | Remarques |
|---|---|---|
| Barre d'outils : propriétés, dossier, nouveau site, supprimer | ✅ | |
| Mode arborescence | ✅ | |
| Mode liste, largeur automatique des colonnes | 🟡 | Mode liste, triable ; pas de largeur automatique des colonnes |
| Import texte brut (une URL par ligne) | ✅ | Détecté par l'import |
| Import des sites et réglages UltraFXP | ⬜ | Pas fait : aucun fichier d'exemple du format |
| Import FTPRush XML | ✅ | Plus FTPRush v1/v3, FileZilla, FlashFXP ; les mots de passe ne sont pas importés |
| Export texte brut / CSV / HTML / FTPRush XML | ✅ | Texte, CSV, HTML, XML FTPRush v2 ; jamais les mots de passe |
| Réglages par défaut d'un site | ✅ | Options › Nouveau site : protocole, connexion, mode, limites, dossiers, listing |
| Réglages de sécurité (mot de passe du carnet, afficher les mots de passe) | ✅ | Mot de passe maître (empreinte salée) pour le gestionnaire, les options et le verrouillage ; mots de passe protégés par DPAPI |
| Tester la disponibilité des sites | ✅ | Un site ou tout un dossier, en parallèle |

### Propriétés d'un site

| Sujet | État | Remarques |
|---|---|---|
| Protocole : FTP simple, `AUTH TLS`, SSL implicite, `AUTH SSL`, SFTP | ✅ | `AUTH SSL` en repli du FTPS explicite |
| Plusieurs entrées hôte:port | ✅ | Rotation en cas d'échec, l'adresse qui a marché sert en premier |
| Port, utilisateur, mot de passe, anonyme | ✅ | |
| Mode passif, repli actif quand `PASV` échoue | ✅ | Repli activé dans la page Connexion |
| Serveur derrière un NAT (adresse de l'hôte pour `PASV`) | ✅ | Option du site ; automatique aussi quand `PASV` renvoie une adresse privée |
| Identifiant en UTF-8, UTF-8 forcé | ✅ | Par site (FTP) |
| Fuseau horaire | ✅ | Fuseau du serveur (UTC−12 à +14) ou décalage ajouté aux dates des listings (FTP) |
| Transferts simultanés maximum (0 = connexion de navigation) | ✅ | |
| Chemins distant et local | ✅ | |
| Description (texte court pour les menus, notes) | ✅ | Affichée à côté du nom dans l'arbre des sites |
| Signets | ✅ | |
| Correspondance d'URL HTTP, copier l'URL HTTP | ✅ | Panneau distant : clic droit → Copier l'adresse HTTP |
| Affichage : nom du site au lieu de [1]/[2] dans le journal | ✅ | Par site, ou pour tous dans Options › Interface |
| Connexion : essais et délais globaux ou propres au site | ✅ | |
| Reprise : auto / oui / non | ✅ | Par site |
| Maintien de connexion par site | ✅ | |
| `FEAT` par site | ✅ | Fonctions utilisées choisies par site (`PRET`, UTF-8, `MODE Z`, `XCRC`, `MLSD`, `SSCN`, `CPSV`) |
| `PRET` | ✅ | Utilisé quand le serveur l'annonce |
| UTF-8 | ✅ | `OPTS UTF8 ON` quand le serveur l'annonce |
| `CLNT` | ✅ | Nom du client par site |
| Compression `MODE Z` | ✅ | Réglage global (Transfert avancé), désactivable par site |
| Vérification `XCRC` après transfert | ✅ | Réglage global (Transfert avancé), désactivable par site |
| `MLSD` | ✅ | |
| `SSCN` / `CPSV` pour le FXP chiffré | ✅ | |
| Comptes : identifiants différents pour envois et téléchargements | ✅ | Avec une adresse et un port à eux ; la navigation garde le compte principal |
| Ident : nom global ou propre au site | ✅ | Répondu pendant la connexion du site |
| Proxy : global, contourné, personnalisé ; canal de données | ✅ | |
| Mode de listing : `STAT -l` | ✅ | Lu sur le canal de contrôle |
| Mode de listing : retirer les espaces de tête | ✅ | Toujours fait pour `STAT` |
| Mode de listing : commande personnalisée | ✅ | |
| Mode de listing : UPnP | ⬜ | |
| Cache des dossiers par site | ✅ | Réglage global, ou oui / non dans la fiche du site |
| Jeu de caractères des dossiers (serveurs non UTF-8) | ✅ | Commandes, réponses et listings ; 14 jeux de caractères |
| Choix de l'analyseur de listing | ✅ | Automatique, Unix ou DOS/IIS (`MLSD` est toujours lu comme tel) |
| SSL : `PBSZ 0` | ✅ | Désactivable par site |
| SSL : chiffrer listings / transferts séparément | ✅ | |
| SSL : FXP chiffré | ✅ | Automatique, jamais ou toujours par site |
| SFTP : clés privées | ✅ | Plusieurs clés, essayées à tour de rôle |
| Transfert : conversion de casse des noms | ✅ | Envoi, réception et FXP ; global ou par site |
| Transfert : XDupe | ✅ | Global ou par site, avec la relecture quand XDUPE ne renvoie rien |
| Transfert : `NOOP` pendant les transferts | ✅ | Global ou par site |
| Transfert : méthode FXP (pas de `PASV` côté source, `PASV` côté cible) | ✅ | Par site ; inversion automatique quand `PORT` est refusé |
| Transfert : forcer le mode actif | ✅ | Mode de transfert de la fiche ; pour tous les sites dans Transfert avancé |
| Transfert : taille du tampon TCP | ✅ | Global, ou tampons d'envoi et de réception par site |
| Liste d'exclusion, priorités, règles « fichier existant » par site | ✅ | Règles « fichier existant » par sens (envoi, réception, FXP), aussi en global |
| Export / import de la liste d'exclusion | ✅ | JSON, ou fichier texte d'un masque par ligne |
| Notification d'événements : surveiller des dossiers | ✅ | Dans la fiche du site ou Options › Notifications ; une connexion à part à chaque vérification |
| Statistiques de ce serveur | ✅ | Dernier accès, connexions, envoyé / reçu / FXP ; copie et remise à zéro des totaux |

## Options

### Général

| Sujet | État | Remarques |
|---|---|---|
| Adresse e-mail pour la connexion anonyme | ✅ | |
| Dossier de téléchargement par défaut | ✅ | Les panneaux locaux y démarrent |
| Dossier des données | ✅ | `-f`, `URUSHFTP_DATA` |
| Sauvegarde quotidienne de la configuration | ✅ | Options › Sécurité ; 4 gardées |
| Cache des dossiers FTP, mise à jour du cache du dossier transféré | ✅ | Options › Navigation |
| Vérifier les nouvelles versions au démarrage | ➖ | Pas de mise à jour en ligne dans cette phase |
| Une seule instance | ✅ | Options › Sécurité |
| Écran de démarrage | ➖ | La fenêtre s'ouvre immédiatement |
| Taille du tampon du journal | ✅ | Options › Interface |
| Journal sur disque | ✅ | Un fichier par jour et par journal |
| Mise à jour groupée du journal | ➖ | L'interface redessine déjà au plus une fois par image |
| Réserve de threads | ➖ | Moteur asynchrone (tokio) ; rien à régler |
| UPnP | ⬜ | |

### Connexion

| Sujet | État | Remarques |
|---|---|---|
| Envoyer `FEAT` | ✅ | Par site (fiche du site) |
| Délai de connexion, délai de réponse | ✅ | |
| Essais maximum, délai entre essais | ✅ | |
| Alterner les IP du serveur en cas d'échec de connexion | ✅ | Autres adresses de la fiche du site |
| Reconnexion automatique sur certaines réponses du serveur | ✅ | Masques à jokers, page Maintien de connexion |
| Gestion des serveurs derrière un NAT | ✅ | |
| Adresse externe annoncée (mode actif) | ✅ | |
| Lier à une adresse IP locale | ✅ | Connexions directes (pas celle du proxy) |
| Plage de ports locaux | ✅ | |
| Ident : activer, nom, système, port | ✅ | |
| Ident : seulement pendant la connexion | ✅ | Les autres requêtes reçoivent `NO-USER` |
| Maintien de connexion : commande, intervalle, arrêt, déconnexion, reconnexion après expulsion, commande aléatoire | ✅ | Global ou par site |
| Proxy : liste de proxys nommés | ✅ | Page Proxy ; choisis par leur nom dans les fiches de site |
| Proxy : `NOOP` pendant les transferts, passif avec proxy | ✅ | `NOOP` : page Connexion ; mode passif toujours utilisé à travers un proxy |
| Proxy : contourner pour les adresses locales, exceptions | ✅ | |
| Éditeur de proxy | ✅ | Intégré aux options et à la fiche du site |
| Mode de listing : afficher les fichiers cachés (`LIST -a`) | ✅ | |
| Mode de listing : commande personnalisée | ✅ | Globale ou par site |
| Mode de listing : passer en `PORT` quand `PASV` échoue | ✅ | Pour les listings et les transferts |
| Mode de listing : utiliser `MLSD` si disponible | ✅ | |

### Transfert

| Sujet | État | Remarques |
|---|---|---|
| Limites de vitesse de réception / d'envoi | ✅ | |
| Pas de limite pour les adresses locales | ✅ | Adresses privées, de bouclage et de lien local |
| Conversion de casse des noms | ✅ | Envois et téléchargements séparément |
| Forcer le mode actif pour les transferts | ✅ | |
| Arrêter et reprendre quand aucune donnée n'arrive | 🟡 | Délai de réponse puis nouvel essai avec reprise |
| Revenir en arrière à la reprise d'un téléchargement | ✅ | |
| Garder un espace disque libre minimum | ✅ | |
| Transferts simultanés maximum par onglet | ✅ | |
| Taille du tampon TCP | ✅ | |
| Mode de transfert auto / binaire / ASCII | ✅ | Voir la ligne du menu |
| À la fin des transferts : rester connecté / déconnecter | ✅ | |
| Liste ASCII, pas de reprise pour les fichiers ASCII | ✅ | |
| Liste d'exclusion : activer, fichiers vides, dossiers vides, dossiers | ✅ | |
| Liste d'exclusion : couleur des éléments exclus | ⬜ | |
| Liste d'exclusion : mode auto / manuel / les deux | ⬜ | Appliquée à tout élément aujourd'hui |
| Règles « fichier existant » (reprendre, ignorer, renommer, écraser, demander) | ✅ | Plus écraser si la reprise échoue |
| File : recharger la file inachevée au démarrage, enregistrement automatique | ✅ | Demander, charger ou abandonner (Options › Général) |
| File : garder les éléments en échec | ✅ | |
| File : nouvel essai cyclique (l'élément en échec passe en bas) | ✅ | |
| File : relancer tous les échecs à la fin de la file | ✅ | Un nombre de tours |
| File : vider en changeant de serveur à la main | ➖ | Pas de changement manuel de serveur entre onglets |
| File : réinitialiser les échecs au démarrage automatique | ✅ | |
| File : vider en déconnectant un site en cours de transfert | ✅ | Ses éléments en attente sont retirés |
| File : source / destination dans le journal des transferts | ✅ | |
| File : nombre d'essais par sens (envoi, réception, FXP) | ✅ | Un réglage du site l'emporte toujours |
| Liste de priorités | ✅ | |
| Compression `MODE Z` (listing, envoi, réception, exceptions) | 🟡 | Envois et téléchargements, avec exceptions ; les listes ne sont pas compressées |
| Cache disque des téléchargements | ➖ | Les écritures sont tamponnées par le moteur |

### Divers

| Sujet | État | Remarques |
|---|---|---|
| Vérifier le SFV au téléchargement, journal CRC et dossier d'état, réajouter les fichiers défectueux | 🟡 | Vérification et remise en file ; résultats au journal, pas de dossier d'état |
| Utiliser `XCRC` si disponible | ✅ | |
| Entrer dans un dossier après l'avoir créé | ✅ | |
| Retirer les dossiers vides laissés par un transfert | ✅ | |
| Refuser un dossier du même nom que son parent | ✅ | |
| Un seul SFV par dossier, ignorer tous les SFV s'il y en a plusieurs | ✅ | |
| Garder la date du serveur sur les fichiers téléchargés | ✅ | |
| Retirer le fichier cible de 0 octet quand la source FXP refuse | ✅ | |
| Nouvel onglet : les deux côtés distants | ⬜ | |
| Nouvel onglet pour la connexion rapide / les favoris | ✅ | « Ouvrir un nouvel onglet quand le panneau est occupé » |

### Affichage

| Sujet | État | Remarques |
|---|---|---|
| Zone de notification : réduire, clignoter pendant les transferts, toujours afficher | 🟡 | Réduire et toujours afficher ; le bouton clignote à la fin d'une file, pas pendant les transferts |
| Fenêtre locale : chemin complet dans la barre d'adresse | ✅ | |
| Fenêtre locale : afficher les fichiers cachés | ⬜ | Les fichiers cachés sont listés aujourd'hui |
| Affichage des tailles : octets / Ko / dynamique | 🟡 | Dynamique seulement |
| Faire clignoter la fenêtre à la fin des transferts | ✅ | Quand la fenêtre est en arrière-plan |
| Actualiser la taille d'un fichier en cours de téléchargement | ⬜ | |
| Menu des favoris : découpage, historique, entrée connexion rapide, sites récents | 🟡 | Sites récents et tous les sites ; pas de menu scindé, pas d'historique des connexions rapides |
| Indicateur de côté en dégradé, masquer la sélection sans focus, séparateur de noms | ➖ | Options cosmétiques de l'interface Delphi |
| Langue | 🟡 | Anglais et français |
| Tri : modes distant et local | ✅ | Clic sur une colonne |
| Tri : liens à part, décroissant par défaut pour date et taille | 🟡 | Plus grand / plus récent d'abord, dossiers d'abord ou non ; les liens ne sont pas à part |
| Surlignage (masques colorés) | ✅ | Une couleur par thème, contraste affiché |
| Polices (application, listes, journal) | 🟡 | Taille de toute l'interface et des journaux ; pas de choix de police |
| Couleurs (lignes, journal, barre de progression), thèmes de couleurs | 🟡 | Thèmes vérifiés pour le contraste ; couleurs des journaux et du surlignage avec leur contraste ; pas de couleur de la barre de progression |
| Disposition : onglets, plusieurs lignes, onglets en bas, largeur | 🟡 | Onglets en haut ou en bas ; une ligne |
| Disposition : barres de titre des panneaux du dock | ➖ | Onglets egui_dock |
| Disposition : bascule par onglets distant/local, grandes icônes | ⬜ | |
| Disposition : options de performance | ➖ | Sans objet avec le rendu GPU |
| Style (thème XP, plat, Office 2003…) | ➖ | Styles de la boîte à outils Delphi |
| Barre d'état : durées de la file, format de la barre de progression (`%p`, `%s`, `%e`, `%c`) | ✅ | `%p` `%b` `%s` `%e` `%c` |

### Souris, associations, événements, confirmations

| Sujet | État | Remarques |
|---|---|---|
| Actions du double-clic (fichier, journal, site, indicateur de côté, onglet, élément de file) | 🟡 | Double-clic sur un fichier : transférer, afficher ou rien ; les autres sont fixes |
| Glisser depuis l'Explorateur vers un panneau distant | ✅ | Dans le dossier du panneau distant sous le pointeur |
| Coller des fichiers locaux dans un panneau distant (`Ctrl+V`) | ✅ | |
| Menu contextuel de l'Explorateur dans le panneau local | ⬜ | |
| Associations de fichiers : voir et modifier les fichiers distants avec des programmes externes | ✅ | Options › Navigation ; Bloc-notes quand aucun programme ne correspond ; un fichier modifié est renvoyé à l'enregistrement |
| Notification d'événements : sons, bulles de notification, liste d'événements | ✅ | Options › Notifications |
| Confirmations : supprimer des fichiers locaux | ✅ | Toujours demandée |
| Confirmations : quitter, éteindre, fermer un onglet connecté ou avec une file, supprimer des éléments de file | ✅ | Quitter, fermer un onglet occupé, retirer des éléments de la file ; l'arrêt a son compte à rebours |

## Scripts

| Sujet | État | Remarques |
|---|---|---|
| Fichiers de script `.PAC` / compilés `.PCC` | 🟡 | Scripts sources (`-s`, console, mIRC) ; pas de forme compilée |
| Identifiants, affectations, chaînes (`#nn`), commentaires | ✅ | |
| Variables (`var`, implicites) | ✅ | |
| Index (`Str[i]`, propriétés tableau) | ✅ | Chaînes à partir de 1, tableaux à partir de 0, `A[i, j]` ; `Selection.Items[i]` |
| Tableaux (constructeurs, plusieurs index) | ✅ | `[…]`, imbriqués, `in`, `SetLength`, `Length`, `Low`, `High` ; pas de `VarArrayCreate` |
| `if`, `while`, `repeat`, `for` | ✅ | |
| `case` | ✅ | Étiquettes de tout type, intervalles |
| Procédures et fonctions de l'utilisateur | ✅ | Paramètres `var`, `Result`, récursivité ; pas imbriquées |
| `RushApp.Language` | ✅ | |
| `RushApp.UI.Bar`, `RushApp.UI.Menu` | ➖ | L'aide n'en décrit aucun membre : un script qui s'en sert reçoit une erreur claire |
| `RushApp.UI.Selection` (`Prepare`, `Count`, `Items`, `IsRemoteSide`, `IsFTPConnected`, `FTPName`) | ✅ | `Date` est un texte |
| `RushApp.FTP.Splitter` | ✅ | |
| `RushApp.FTP.CWD`, `Delete`, `Login`, `Logout`, `RAW`, `RemoveQueue` | ✅ | Avec leurs drapeaux |
| `RushApp.FTP.Transfer` | 🟡 | Tous les paramètres sauf `RS_CHECKLOGIN` ; `Timeout` n'est pas utilisé, comme dans FTPRush |
| `RushApp.FTP.EnableSite`, `DisableSite` | ✅ | |
| Événement `RushApp.UI.OnSwitchLayout` | ✅ | Au chargement d'une disposition enregistrée |
| Variables de statistiques (`TotalDownloadSize`… `DownloadBW`) | ✅ | |
| `RushApp.DeleteEmptyTabs` | ✅ | |
| `MatchExpr`, `MatchString` | ✅ | Sans tenir compte de la casse, comme les filtres de transfert |
| `ShellExecuteCommand` | ✅ | Demandé à chaque fois par défaut (Options › Sécurité) ; `Flag` non utilisé |
| Exemple : `InputQuery` pour un signet | ✅ | Boîte de saisie ; les scripts tournent à côté de l'interface |
| Préfixes `%F%` / `%D%` dans les filtres de transfert | ✅ | |

## Avancé, FAQ et astuces

| Sujet | État | Remarques |
|---|---|---|
| Ligne de commande `-d`, `-w`, `-c`, `-f`, `-r[t][x]` | ✅ | |
| Ligne de commande `-b` (licence bêta) | ➖ | Pas de licence dans UltraRushFTP ; accepté et ignoré |
| Jokers `*` `?` (sans casse) | ✅ | |
| Expressions régulières | 🟡 | Syntaxe de `regex` (Rust), proche mais pas identique à TRegExpr |
| FAQ : impossible de supprimer un dossier (fichiers cachés) | ✅ | Page Connexion › Afficher les fichiers cachés |
| FAQ : « Protocol not supported (10043) » | ➖ | Pas de DLL OpenSSL : le TLS est intégré (rustls) |
| FAQ : noms de fichiers multilingues | ✅ | UTF-8 |
| FAQ : limiter les connexions d'un site | ✅ | |
| Astuce : `Maj` + connexion = connexion fantôme `!user` | ✅ | Gestionnaire de sites ; `RS_GHOST` dans les scripts |
| Astuce : `Ctrl` + connexion = `-motdepasse` | ✅ | Gestionnaire de sites ; `RS_INVERT` dans les scripts |
| Astuce : `Maj` + signet = les deux côtés | ✅ | Menu Signets du panneau distant |
| Astuce : `Maj` + double-clic sur un dossier = les deux côtés | ✅ | |
| Astuce : `%F%` / `%D%` dans les listes d'exclusion et d'autorisation de `Transfer` | ✅ | |
| DLL mIRC (`rushmirc.dll`, `Invoke`, `RushScript`…) | ✅ | 32 et 64 bits |

## Hors aide

Pages sans fonction : accueil, remerciements, support, titres de menus (8 pages).
UltraRushFTP ajoute ce que FTPRush v2 n'avait pas : thème sombre, navigation au clavier et
noms pour lecteurs d'écran contrôlés par des tests, mots de passe protégés par DPAPI,
imports depuis FileZilla et FlashFXP, l'outil en ligne de commande `urftp`, les connexions
IPv4/IPv6 « happy eyeballs » et le FXP entre serveurs IPv6 (`EPSV` / `EPRT`).
