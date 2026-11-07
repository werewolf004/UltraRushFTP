# Parité avec FTPRush v2

[English](PARITY.md) | **Français**

Chaque sujet de l'aide de FTPRush v2 (235 pages, lues en entier le 2026-11-06) et
l'état d'UltraRushFTP. L'aide sert uniquement de référence fonctionnelle : rien n'en est
copié et FTPRush lui-même n'est pas décompilé.

Légende : ✅ fait · 🟡 en partie · ⬜ prévu · ➖ hors périmètre (raison donnée).

Résumé (284 sujets) : ✅ 110 · 🟡 31 · ⬜ 126 · ➖ 17.

## Fenêtre principale et menus

### Fichier

| Sujet | État | Remarques |
|---|---|---|
| Nouvelle fenêtre | ✅ | Nouvel onglet (`+`, `Ctrl+T`) |
| Nouvelle fenêtre à partir de l'actuelle | ⬜ | Dupliquer un onglet avec ses connexions |
| Fermer la fenêtre courante | ✅ | `✕`, `Ctrl+W` (déconnecte) |
| Réduire dans la zone de notification | ⬜ | Icône de notification |
| Verrouiller dans la zone de notification | ⬜ | Demande un mot de passe sur le carnet de sites (voir *Réglages de sécurité*) |
| Sauvegarder / restaurer la configuration | ⬜ | Une archive avec sites, options et statistiques |
| Quitter | ✅ | |

### Édition et marquage

| Sujet | État | Remarques |
|---|---|---|
| Recherche (filtre rapide) | ⬜ | Filtre d'un panneau par `Ctrl+F`, mots, `-mot`, expression régulière |
| Renommer | ✅ | `F2` |
| Supprimer | ✅ | `Suppr`, avec confirmation |
| Créer un dossier | ✅ | `F7` |
| Renommer plusieurs fichiers | ⬜ | |
| Copier les noms sélectionnés | 🟡 | « Copier le chemin » pour un élément ; pas encore plusieurs noms |
| Marquer / démarquer, tout démarquer, sélectionner les marqués, exporter les marques | ⬜ | Marques distinctes de la sélection |
| Sélectionner (par masque) | ⬜ | |
| Tout sélectionner | ✅ | `Ctrl+A` |

### Affichage

| Sujet | État | Remarques |
|---|---|---|
| Actualiser | ✅ | `F5` |
| Liste des fenêtres | 🟡 | Barre d'onglets seulement ; pas de liste avec colonnes connexion et action |
| Fenêtre de file d'attente | ✅ | Panneau du dock |
| Journal système, journal des transferts, journal FTP | 🟡 | Un journal par onglet, couleurs commande / réponse / état / erreur |
| Fenêtre des tâches | ⬜ | Tâches planifiées |
| Colonnes distantes taille / date | ✅ | |
| Colonnes distantes attributs / utilisateur / groupe | ⬜ | Lues par l'analyse du listing, non affichées |
| Colonnes locales taille / date | ✅ | |
| Colonnes de la liste des fenêtres | ⬜ | |
| Lignes de grille | ➖ | Lignes alternées à la place |
| Ajustement automatique des colonnes | 🟡 | Colonnes redimensionnables, sans ajustement automatique |
| Fichiers en cours d'édition | ⬜ | Vient avec les *Associations de fichiers* |

### FTP

| Sujet | État | Remarques |
|---|---|---|
| Boîte de connexion rapide | ✅ | Barre de connexion rapide |
| Attributs (CHMOD) | ⬜ | `SITE CHMOD` |
| Déplacer vers | ⬜ | `RNFR`/`RNTO` distant vers un autre dossier |
| Modifier la date de fichiers FTP | ⬜ | `MFMT` / `SITE UTIME` |
| Calculer l'espace FTP utilisé | ⬜ | Taille récursive avec profondeur |
| Exporter l'arborescence | ⬜ | |
| Chercher des fichiers sur les sites | ⬜ | Recherche récursive avec filtres |
| Vider le cache du site / de tous les sites | ⬜ | Vient avec le cache des dossiers |

### Outils → File d'attente

| Sujet | État | Remarques |
|---|---|---|
| Démarrer / mettre en pause la file | 🟡 | Démarrer et arrêter ; pas de pause qui garde les connexions |
| Ajouter la sélection à la file | ✅ | Bouton Transférer, `Espace`, glisser-déposer |
| Transfert conditionnel | ⬜ | Masques inclure/exclure, dates, tailles, synchronisation |
| Vider la file courante | 🟡 | « Retirer les terminés » et retrait d'un élément ; pas « tous les éléments en attente » |
| Monter / descendre / tout en haut / tout en bas | ✅ | Clic droit |
| Modifier un élément de la file | ⬜ | Changer source, cible, mode ; inverser un FXP |
| Relancer tous les échecs | ✅ | « Relancer les échecs » |
| Relancer les échecs sélectionnés | ✅ | Clic droit → Relancer |
| Retirer tous les échecs | ⬜ | |
| Marquer la sélection / tout en échec | ⬜ | |
| Informations sur la file | ⬜ | Nombres, tailles, durées |
| Restaurer la file, charger / enregistrer un fichier de file | ⬜ | |
| Planifier la file en tâche | ⬜ | |
| Bascules de limite de vitesse d'envoi / de réception | 🟡 | Limites dans les options ; pas de bascule en un clic |

### Outils → autres

| Sujet | État | Remarques |
|---|---|---|
| Gestionnaire de sites | ✅ | |
| Ajouter au gestionnaire de sites | ⬜ | Enregistrer une connexion rapide comme site |
| Commandes (commandes de site prédéfinies) | ⬜ | Menu de commandes `SITE` selon le type de serveur |
| Concepteur de commandes | ⬜ | |
| Envoyer une commande RAW | 🟡 | `RushApp.FTP.RAW` depuis la console de script ; pas de boîte de dialogue ni « envoyer à tous » |
| Comparer le contenu des dossiers (nom, taille, partiel, masquer les identiques) | ⬜ | |
| Voir le listing brut | ⬜ | Listing non analysé |
| Concepteur de scripts | 🟡 | Console de script dans chaque onglet ; pas d'éditeur avec fichiers |
| Langues | 🟡 | Anglais et français |
| Gestionnaire de raccourcis | ⬜ | Raccourcis fixes (liste : `F1`) |
| Téléchargement manuel depuis un site, liste d'URL incrémentées | ⬜ | |
| Surveiller le presse-papiers, transfert automatique | ⬜ | |
| Mode de transfert auto / binaire / ASCII | 🟡 | `TYPE A` géré par la couche protocole ; pas de réglage ni de liste ASCII |
| À la fin des transferts : déconnecter / rester connecté | ⬜ | |
| À la fin des transferts : éteindre / veille / veille prolongée | ⬜ | |
| À la fin des transferts : quitter | 🟡 | Ligne de commande `-rx` seulement |
| Créer un fichier SFV ou MD5 | ⬜ | |
| Traduction d'adresse IP | ⬜ | Adresse hexadécimale / numérique vers la forme pointée |
| Redémarrer le service UPnP | ⬜ | Vient avec UPnP |
| Options | ✅ | Fenêtre Options |

### Aide

| Sujet | État | Remarques |
|---|---|---|
| Index de l'aide | 🟡 | `F1` liste les raccourcis ; documentation complète dans `docs/` |
| Assistant FTPRush | ➖ | Assistant de personnalisation propre à la boîte à outils Delphi |
| Vérifier les nouvelles versions | ➖ | Pas de mise à jour en ligne dans cette phase |
| À propos | ⬜ | Version, licence, mentions |

### Interface principale et prise en main

| Sujet | État | Remarques |
|---|---|---|
| Disposition principale (panneaux, journaux, file, barre d'état) | 🟡 | Dock avec panneaux, file, journal, script ; un seul journal, pas de fenêtre des tâches |
| Indicateur de côté | 🟡 | `▸` sur le panneau actif |
| Se connecter depuis le menu des favoris | ⬜ | Menu déroulant des sites dans la barre d'outils |
| Se déconnecter d'un site | ✅ | |
| Verrouiller / déverrouiller barre d'outils et dock | ➖ | Le dock se réorganise par glisser ; pas de barre d'outils à verrouiller |
| Connexion rapide à un site | ✅ | `Tab` entre les champs, `Entrée` connecte |
| Personnaliser l'interface (styles) | ➖ | Styles de la boîte à outils Delphi (XP, Office 2003…) ; thèmes sombre et clair à la place |

## Boîtes de dialogue

| Sujet | État | Remarques |
|---|---|---|
| Ajouter ou modifier une règle de la liste d'exclusion | ✅ | Modifiée dans la grille des options |
| Signets | 🟡 | Nom, chemins distant et local ; jetons de date (`%yyyy%`, `%mm%`…) pas encore |
| Créer des sommes de contrôle CRC | ⬜ | |
| Transfert conditionnel | ⬜ | Voir *Transfert conditionnel* |
| Calculer l'espace FTP utilisé | ⬜ | |
| Modifier un élément de la file | ⬜ | |
| Exporter l'arborescence | ⬜ | |
| Recherche rapide | ⬜ | |
| Chercher des fichiers sur les sites | ⬜ | |
| Gestionnaire de raccourcis | ⬜ | |
| Boîte RAW (`Ctrl+R`, plusieurs lignes, tous les sites) | ⬜ | |
| Téléchargement manuel | ⬜ | |
| URL incrémentées | ⬜ | |

## Gestionnaire de sites

| Sujet | État | Remarques |
|---|---|---|
| Barre d'outils : propriétés, dossier, nouveau site, supprimer | ✅ | |
| Mode arborescence | ✅ | |
| Mode liste, largeur automatique des colonnes | ⬜ | |
| Import texte brut (une URL par ligne) | ⬜ | |
| Import des sites et réglages UltraFXP | ⬜ | |
| Import FTPRush XML | ✅ | Plus FTPRush v1/v3, FileZilla, FlashFXP ; les mots de passe ne sont pas importés |
| Export texte brut / CSV / HTML / FTPRush XML | ⬜ | Sans les mots de passe |
| Réglages par défaut d'un site | ✅ | Options › Nouveau site : protocole, connexion, mode, limites, dossiers, listing |
| Réglages de sécurité (mot de passe du carnet, afficher les mots de passe) | 🟡 | Mots de passe protégés par DPAPI de Windows ; pas de mot de passe maître ni de verrouillage |
| Tester la disponibilité des sites | ✅ | Un site ou tout un dossier, en parallèle |

### Propriétés d'un site

| Sujet | État | Remarques |
|---|---|---|
| Protocole : FTP simple, `AUTH TLS`, SSL implicite, `AUTH SSL`, SFTP | ✅ | `AUTH SSL` en repli du FTPS explicite |
| Plusieurs entrées hôte:port | ✅ | Rotation en cas d'échec, l'adresse qui a marché sert en premier |
| Port, utilisateur, mot de passe, anonyme | ✅ | |
| Mode passif, repli actif quand `PASV` échoue | ✅ | Repli activé dans la page Connexion |
| Serveur derrière un NAT (adresse de l'hôte pour `PASV`) | ✅ | Automatique quand `PASV` renvoie une adresse privée |
| Fuseau horaire | ✅ | Décalage ajouté aux dates des listings (FTP) |
| Transferts simultanés maximum (0 = connexion de navigation) | ✅ | |
| Chemins distant et local | ✅ | |
| Description (texte court pour les menus, notes) | ✅ | Affichée à côté du nom dans l'arbre des sites |
| Signets | ✅ | |
| Correspondance d'URL HTTP, copier l'URL HTTP | ✅ | Panneau distant : clic droit → Copier l'adresse HTTP |
| Affichage : nom du site au lieu de [1]/[2] dans le journal | ⬜ | Le journal affiche Gauche / Droite |
| Connexion : essais et délais globaux ou propres au site | ✅ | |
| Reprise : auto / oui / non | ✅ | Active (le refus du serveur est détecté) ou désactivée par site |
| Maintien de connexion par site | ✅ | |
| `FEAT` par site | ✅ | |
| `PRET` | ✅ | Utilisé quand le serveur l'annonce |
| UTF-8 | ✅ | `OPTS UTF8 ON` quand le serveur l'annonce |
| `CLNT` | ✅ | Nom du client par site |
| Compression `MODE Z` | ⬜ | |
| Vérification `XCRC` après transfert | ⬜ | |
| `MLSD` | ✅ | |
| `SSCN` / `CPSV` pour le FXP chiffré | ✅ | |
| Comptes : identifiants différents pour envois et téléchargements | ✅ | La navigation garde le compte principal |
| Ident : nom global ou propre au site | ✅ | Répondu pendant la connexion du site |
| Proxy : global, contourné, personnalisé ; canal de données | ✅ | |
| Mode de listing : `STAT -l` | ✅ | Lu sur le canal de contrôle |
| Mode de listing : retirer les espaces de tête | ✅ | Toujours fait pour `STAT` |
| Mode de listing : commande personnalisée | ✅ | |
| Mode de listing : UPnP | ⬜ | |
| Cache des dossiers par site | ⬜ | |
| Jeu de caractères des dossiers (serveurs non UTF-8) | ✅ | Commandes, réponses et listings ; 14 jeux de caractères |
| Choix de l'analyseur de listing | ✅ | Automatique, Unix ou DOS/IIS (`MLSD` est toujours lu comme tel) |
| SSL : `PBSZ 0` | ✅ | |
| SSL : chiffrer listings / transferts séparément | ✅ | |
| SSL : FXP chiffré | ✅ | |
| SFTP : clés privées | ✅ | Plusieurs clés, essayées à tour de rôle |
| Transfert : conversion de casse des noms | ⬜ | |
| Transfert : XDupe | ⬜ | |
| Transfert : méthode FXP (pas de `PASV` côté source, `PASV` côté cible) | ✅ | Par site ; inversion automatique quand `PORT` est refusé |
| Transfert : forcer le mode actif | ⬜ | |
| Transfert : taille du tampon TCP | ⬜ | |
| Liste d'exclusion, priorités, règles « fichier existant » par site | ✅ | |
| Notification d'événements : surveiller des dossiers | ⬜ | Demande le maintien de connexion |
| Statistiques de ce serveur | ⬜ | Statistiques globales et de session disponibles |

## Options

### Général

| Sujet | État | Remarques |
|---|---|---|
| Adresse e-mail pour la connexion anonyme | ✅ | |
| Dossier de téléchargement par défaut | ✅ | Les panneaux locaux y démarrent |
| Dossier des données | ✅ | `-f`, `URUSHFTP_DATA` |
| Sauvegarde quotidienne de la configuration | ⬜ | |
| Cache des dossiers FTP, mise à jour du cache du dossier transféré | ⬜ | |
| Vérifier les nouvelles versions au démarrage | ➖ | Pas de mise à jour en ligne dans cette phase |
| Une seule instance | ⬜ | |
| Écran de démarrage | ➖ | La fenêtre s'ouvre immédiatement |
| Taille du tampon du journal | 🟡 | Fixée à 5 000 lignes |
| Journal sur disque | ⬜ | |
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
| Pas de limite pour les adresses locales | ⬜ | |
| Conversion de casse des noms | ⬜ | |
| Forcer le mode actif pour les transferts | ⬜ | |
| Arrêter et reprendre quand aucune donnée n'arrive | 🟡 | Délai de réponse puis nouvel essai avec reprise |
| Revenir en arrière à la reprise d'un téléchargement | ⬜ | |
| Garder un espace disque libre minimum | ⬜ | |
| Transferts simultanés maximum par onglet | ✅ | |
| Taille du tampon TCP | ⬜ | |
| Mode de transfert auto / binaire / ASCII | 🟡 | Voir la ligne du menu |
| À la fin des transferts : rester connecté / déconnecter | ⬜ | |
| Liste ASCII, pas de reprise pour les fichiers ASCII | ⬜ | |
| Liste d'exclusion : activer, fichiers vides, dossiers vides, dossiers | ✅ | |
| Liste d'exclusion : couleur des éléments exclus | ⬜ | |
| Liste d'exclusion : mode auto / manuel / les deux | ⬜ | Appliquée à tout élément aujourd'hui |
| Règles « fichier existant » (reprendre, ignorer, renommer, écraser, demander) | ✅ | Plus écraser si la reprise échoue |
| File : recharger la file inachevée au démarrage, enregistrement automatique | ⬜ | |
| File : garder les éléments en échec | ✅ | |
| File : nouvel essai cyclique (l'élément en échec passe en bas) | ⬜ | |
| File : relancer tous les échecs à la fin de la file | ⬜ | |
| File : vider en changeant de serveur à la main | ➖ | Pas de changement manuel de serveur entre onglets |
| File : réinitialiser les échecs au démarrage automatique | ⬜ | |
| File : vider en déconnectant un site en cours de transfert | ⬜ | |
| File : source / destination dans le journal des transferts | ⬜ | |
| File : nombre d'essais par sens (envoi, réception, FXP) | 🟡 | Une seule valeur pour tous |
| Liste de priorités | ✅ | |
| Compression `MODE Z` (listing, envoi, réception, exceptions) | ⬜ | |
| Cache disque des téléchargements | ➖ | Les écritures sont tamponnées par le moteur |

### Divers

| Sujet | État | Remarques |
|---|---|---|
| Vérifier le SFV au téléchargement, journal CRC et dossier d'état, réajouter les fichiers défectueux | ⬜ | |
| Utiliser `XCRC` si disponible | ⬜ | |
| Entrer dans un dossier après l'avoir créé | ⬜ | |
| Retirer les dossiers vides laissés par un transfert | ⬜ | |
| Refuser un dossier du même nom que son parent | ⬜ | |
| Un seul SFV par dossier, ignorer tous les SFV s'il y en a plusieurs | ⬜ | |
| Garder la date du serveur sur les fichiers téléchargés | ⬜ | |
| Retirer le fichier cible de 0 octet quand la source FXP refuse | ⬜ | |
| Nouvel onglet : les deux côtés distants | ⬜ | |
| Nouvel onglet pour la connexion rapide / les favoris | ✅ | « Ouvrir un nouvel onglet quand le panneau est occupé » |

### Affichage

| Sujet | État | Remarques |
|---|---|---|
| Zone de notification : réduire, clignoter pendant les transferts, toujours afficher | ⬜ | |
| Fenêtre locale : chemin complet dans la barre d'adresse | ✅ | |
| Fenêtre locale : afficher les fichiers cachés | ⬜ | Les fichiers cachés sont listés aujourd'hui |
| Affichage des tailles : octets / Ko / dynamique | 🟡 | Dynamique seulement |
| Faire clignoter la fenêtre à la fin des transferts | ⬜ | |
| Actualiser la taille d'un fichier en cours de téléchargement | ⬜ | |
| Menu des favoris : découpage, historique, entrée connexion rapide, sites récents | ⬜ | |
| Indicateur de côté en dégradé, masquer la sélection sans focus, séparateur de noms | ➖ | Options cosmétiques de l'interface Delphi |
| Langue | 🟡 | Anglais et français |
| Tri : modes distant et local | ✅ | Clic sur une colonne |
| Tri : liens à part, décroissant par défaut pour date et taille | ⬜ | |
| Surlignage (masques colorés) | ⬜ | |
| Polices (application, listes, journal) | ⬜ | |
| Couleurs (lignes, journal, barre de progression), thèmes de couleurs | 🟡 | Thèmes sombre et clair contrôlés pour le contraste ; pas de couleurs personnalisées |
| Disposition : onglets, plusieurs lignes, onglets en bas, largeur | 🟡 | Onglets sur une ligne en haut |
| Disposition : barres de titre des panneaux du dock | ➖ | Onglets egui_dock |
| Disposition : bascule par onglets distant/local, grandes icônes | ⬜ | |
| Disposition : options de performance | ➖ | Sans objet avec le rendu GPU |
| Style (thème XP, plat, Office 2003…) | ➖ | Styles de la boîte à outils Delphi |
| Barre d'état : durées de la file, format de la barre de progression (`%p`, `%s`, `%e`, `%c`) | 🟡 | Onglets, connexions et files actives ; pas de durées ni de texte de progression |

### Souris, associations, événements, confirmations

| Sujet | État | Remarques |
|---|---|---|
| Actions du double-clic (fichier, journal, site, indicateur de côté, onglet, élément de file) | 🟡 | Fixe : ouvre les dossiers ; pas de choix |
| Glisser depuis l'Explorateur vers un panneau distant | ⬜ | Le glisser-déposer marche entre les deux panneaux |
| Coller des fichiers locaux dans un panneau distant (`Ctrl+V`) | ⬜ | |
| Menu contextuel de l'Explorateur dans le panneau local | ⬜ | |
| Associations de fichiers : voir et modifier les fichiers distants avec des programmes externes | ⬜ | |
| Notification d'événements : sons, bulles de notification, liste d'événements | ⬜ | |
| Confirmations : supprimer des fichiers locaux | ✅ | Toujours demandée |
| Confirmations : quitter, éteindre, fermer un onglet connecté ou avec une file, supprimer des éléments de file | ⬜ | |

## Scripts

| Sujet | État | Remarques |
|---|---|---|
| Fichiers de script `.PAC` / compilés `.PCC` | 🟡 | Scripts sources (`-s`, console, mIRC) ; pas de forme compilée |
| Identifiants, affectations, chaînes (`#nn`), commentaires | ✅ | |
| Variables (`var`, implicites) | ✅ | |
| Index (`Str[i]`, propriétés tableau) | ⬜ | |
| Tableaux (constructeurs, plusieurs index) | ⬜ | |
| `if`, `while`, `repeat`, `for` | ✅ | |
| `case` | ⬜ | |
| Procédures et fonctions de l'utilisateur | ⬜ | Refusées aujourd'hui avec une erreur claire |
| `RushApp.Language` | ✅ | |
| `RushApp.UI.Bar`, `RushApp.UI.Menu` | ⬜ | |
| `RushApp.UI.Selection` (`Prepare`, `Count`, `Items`, `IsRemoteSide`, `IsFTPConnected`, `FTPName`) | ⬜ | |
| `RushApp.FTP.Splitter` | ✅ | |
| `RushApp.FTP.CWD`, `Delete`, `Login`, `Logout`, `RAW`, `RemoveQueue` | ✅ | Avec leurs drapeaux |
| `RushApp.FTP.Transfer` | 🟡 | Drapeaux lus ; `RS_NOCIRCLE`, `CompleteFlag`, `RepeatCount`, `Depth`, `SubCount`, tri et filtres dossiers/fichiers pas encore appliqués |
| `RushApp.FTP.EnableSite`, `DisableSite` | ✅ | |
| Événement `RushApp.UI.OnSwitchLayout` | ⬜ | Demande les dispositions enregistrées |
| Variables de statistiques (`TotalDownloadSize`… `DownloadBW`) | ✅ | |
| `RushApp.DeleteEmptyTabs` | ✅ | |
| `MatchExpr`, `MatchString` | ⬜ | |
| `ShellExecuteCommand` | ⬜ | Demande une règle de confirmation avant de lancer des programmes |
| Exemple : `InputQuery` pour un signet | 🟡 | `InputQuery` accepté ; pas encore de boîte de saisie |
| Préfixes `%F%` / `%D%` dans les filtres de transfert | ⬜ | |

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
| Astuce : `Maj` + signet = les deux côtés | ⬜ | |
| Astuce : `Maj` + double-clic sur un dossier = les deux côtés | ⬜ | |
| Astuce : `%F%` / `%D%` dans les listes d'exclusion et d'autorisation de `Transfer` | ⬜ | |
| DLL mIRC (`rushmirc.dll`, `Invoke`, `RushScript`…) | ✅ | 32 et 64 bits |

## Hors aide

Pages sans fonction : accueil, remerciements, support, titres de menus (8 pages).
UltraRushFTP ajoute ce que FTPRush v2 n'avait pas : thème sombre, navigation au clavier et
noms pour lecteurs d'écran contrôlés par des tests, mots de passe protégés par DPAPI,
imports depuis FileZilla et FlashFXP, l'outil en ligne de commande `urftp`, les connexions
IPv4/IPv6 « happy eyeballs » et le FXP entre serveurs IPv6 (`EPSV` / `EPRT`).
