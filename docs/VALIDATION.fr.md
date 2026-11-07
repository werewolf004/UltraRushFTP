*[English](VALIDATION.md)*

# Validation sur de vrais serveurs

Les tests automatiques tournent contre des serveurs écrits pour les tests. Ce guide vérifie
UltraRushFTP face à de vrais logiciels serveurs : glftpd, vsftpd, ProFTPD, FileZilla Server,
IIS et OpenSSH. Il a deux parties :

1. un **script** qui déroule un scénario fixe avec `urftp` (le client en ligne de commande)
   et écrit un rapport anonymisé ;
2. une **liste de contrôle** de ce que le script ne peut pas vérifier, à faire à la main dans
   l'interface.

Utilisez des serveurs ou des comptes de test : le script crée puis supprime un dossier sur
le serveur.

## 1. Le script

### Avant de commencer

- Windows PowerShell 7 (`pwsh`). Le script se trouve dans `scripts/` des sources et à la
  racine de l'archive de la version, à côté de `urftp.exe`.
- Dans le gestionnaire de sites de l'interface, créez le site et **saisissez son mot de
  passe** (le script ne prend jamais d'identifiants sur sa ligne de commande). Connectez-vous
  une fois depuis l'interface pour accepter le certificat ou la clé d'hôte SSH, ou donnez
  `-Insecure` / `-Fingerprint`.
- Un dossier distant où ce compte peut créer des dossiers, envoyer, télécharger et supprimer.

### Le lancer

```powershell
pwsh scripts/validate-server.ps1 -Site "Lab vsftpd" -Folder /upload -Label "vsftpd 3.0.5"
```

Avec un FXP vers un second site :

```powershell
pwsh scripts/validate-server.ps1 -Site "Lab glftpd" -Folder /incoming -FxpTo "Lab ProFTPD" -FxpFolder /pub -Label glftpd
```

| Paramètre | Rôle |
|---|---|
| `-Site` | Site du gestionnaire à vérifier (nom, sans son groupe). |
| `-Folder` | Dossier distant où le dossier de test `urftp-validation-<date>` est créé. |
| `-FxpTo`, `-FxpFolder` | Second site et son dossier : le dossier de test y est copié par FXP, puis relu. |
| `-Label` | Nom du serveur dans le rapport (par exemple son logiciel et sa version). |
| `-Insecure` | Accepter tout certificat TLS (serveurs de test auto-signés). |
| `-Fingerprint` | Empreinte SHA-256 attendue (certificat FTPS ou clé d'hôte SSH). |
| `-SizeMiB` | Taille du gros fichier de test (8 par défaut). |
| `-Keep` | Laisser les fichiers de test sur le serveur. |
| `-Data` | Dossier des réglages (comme `-f` de l'interface). |
| `-Urftp` | Chemin de `urftp.exe` (par défaut : à côté du script, sinon `target/release`). |
| `-Out` | Dossier du rapport (par défaut : `target/validation/<date>` ou `validation/<date>`). |
| `-SelfTest` | Déroule le scénario sur deux serveurs de développement locaux, pour vérifier le script lui-même. |

### Ce qu'il vérifie

| Étape | Vérifié |
|---|---|
| Connexion et liste du dossier | Identification, TLS, liste du dossier. |
| Informations du serveur | `SYST`, `FEAT`, `PWD` (FTP seulement ; un refus est un avertissement). |
| Envoi d'un dossier | `MKD`, trois envois simultanés, sous-dossiers, noms avec espaces, accents et `#`. |
| Liste du dossier envoyé | La liste montre les noms, les accents et les dossiers. |
| Téléchargement du dossier | Trois téléchargements simultanés ; chaque fichier comparé par SHA-256. |
| Reprise d'un téléchargement | 40 % du fichier présent en local : `REST` puis la suite, fichier identique. |
| Écrasement à l'envoi | Le fichier distant est remplacé. |
| Reprise d'un envoi | 40 % du fichier sur le serveur : `REST`+`STOR` ou `APPE`, relu et comparé. |
| FXP et relecture | Copie site à site du dossier, puis téléchargement depuis le second site et comparaison. |
| Suppression des fichiers de test | `DELE` et `RMD` (FTP et FTPS ; en SFTP, supprimez le dossier à la main). |

Une étape est **ok**, **fail** (échec), **warn** (fonctionne, mais pas comme prévu : par
exemple une reprise qui a retéléchargé tout le fichier) ou **skip** (sautée). Le script
se termine avec le code 1 si une étape a échoué.

### Le rapport

Le dossier du rapport contient `report.md` (le tableau des résultats) et `log.txt` (toutes
les commandes et réponses, de toutes les connexions). Avant de les écrire, le script masque
l'hôte, le nom d'utilisateur, les noms des sites, toutes les adresses IPv4 et IPv6, les
n-uplets `PASV`/`PORT` et le dossier de l'utilisateur Windows ; les mots de passe
n'apparaissent jamais (`PASS ****`). **Relisez les deux fichiers avant de les envoyer** :
une bannière d'accueil peut encore nommer le site.

## 2. À la main dans l'interface

Cochez ce qui fonctionne, notez ce qui ne fonctionne pas, avec le journal du protocole
(sélectionnez les lignes du panneau *Journal* à la souris, puis `Ctrl+C`).

- [ ] **Mode actif** : site en mode actif (`PORT`/`EPRT`), avec et sans l'adresse externe
      et la plage de ports d'*Options → Connexion* ; liste et téléchargement.
- [ ] **PASV refusé** : avec *Passer en mode actif (PORT) quand le mode passif échoue* coché, un serveur ou un pare-feu qui refuse
      `PASV`.
- [ ] **TLS** : canal de données chiffré (`PROT P`) et en clair (`PROT C`) ; listes
      chiffrées à part ; changement de certificat détecté (empreinte).
- [ ] **FXP avec TLS** : `SSCN` ou `CPSV` entre deux serveurs TLS ; cases *FXP depuis ce site : jamais PASV* /
      *FXP vers ce site : toujours PASV* du site.
- [ ] **Listes** : `MLSD` désactivé puis activé ; `STAT -l` ; `LIST -a` (fichiers cachés) ;
      décalage horaire ; jeu de caractères d'un serveur non UTF-8 (noms en Windows-1252 par
      exemple).
- [ ] **Maintien de connexion** : laisser une connexion inactive au-delà du délai du
      serveur ; les commandes de maintien partent, ou la reconnexion a lieu.
- [ ] **Expulsion / reconnexion** : couper la session côté serveur pendant un transfert ;
      le transfert reprend.
- [ ] **Ident** : serveur Ident actif, le serveur le demande (glftpd) ; avec et sans *Répondre
      seulement pendant l'ouverture d'une connexion*.
- [ ] **Connexion fantôme** (`!user`, `Maj`+connexion) et `-password` (`Ctrl`+connexion)
      sur glftpd.
- [ ] **Plusieurs hôtes** : premier hôte faux, le suivant est utilisé.
- [ ] **Proxy** : SOCKS5 et HTTP CONNECT, proxy nommé du site.
- [ ] **File non terminée** : arrêter un téléchargement de plusieurs fichiers, fermer
      l'application (ou terminer son processus), relancer : la file est reproposée, onglet
      par onglet.
- [ ] **FTPRush v2 à côté** : UltraRushFTP ouvert, lancer FTPRush v2 : les deux tournent ;
      la DLL mIRC dialogue avec UltraRushFTP.
- [ ] **SFTP** : mot de passe, clé, clé avec phrase secrète, Pageant, plusieurs clés ; clé
      d'hôte changée sur le serveur : refus avec son empreinte.

## 3. Ce qu'il faut regarder sur chaque serveur

| Serveur | Réglages à essayer | Comportements connus à vérifier |
|---|---|---|
| **glftpd** | FTPS explicite, TLS exigé pour les données, `-Insecure` pour son certificat auto-signé. | Reprise de session TLS sur le canal de données ; listes `STAT -l` ; `SSCN` pour le FXP entre sites glftpd ; Ident ; `!user` et `-password` ; commandes `SITE` via `urftp raw`. |
| **vsftpd** | `ssl_enable=YES`, `require_ssl_reuse=YES` (défaut), `pasv_promiscuous=YES` et `port_promiscuous=YES` pour le FXP. | Connexion de données refusée sans reprise de session TLS ; `hide_ids` (propriétaire `ftp`) ; pas de `MLSD` (repli sur `LIST`). |
| **ProFTPD** | `mod_tls` avec `TLSRequired on` ; `AllowForeignAddress on` pour le FXP. | Reprise de session TLS (sans `NoSessionReuseRequired`) ; `MLSD` ; `SITE CHMOD`. |
| **FileZilla Server 1.x** | FTPS explicite et implicite, TLS 1.2 / 1.3. | Reprise de session TLS exigée ; FXP refusé par défaut (`PORT` vers une autre adresse) ; `MLSD` ; noms UTF-8. |
| **IIS FTP** | *Style de liste des répertoires* Unix **et** MS-DOS ; *Exiger SSL*. | Format `LIST` Windows (`11-07-26  08:55PM  <DIR>`) ; `SYST` = `Windows_NT` ; dossiers virtuels ; chemins longs. |
| **OpenSSH** | `Subsystem sftp internal-sftp`, comptes à mot de passe et à clé, `ChrootDirectory`. | Clé d'hôte à la première connexion ; clés `ed25519`, `rsa-sha2-512`, `ecdsa` ; reprise (écriture à un décalage) ; liens symboliques. |

## Résultats

| Serveur et version | Protocole | Script | À la main | Notes |
|---|---|---|---|---|
| | | | | |

Envoyez `report.md`, `log.txt` et ce tableau (après les avoir relus). Chaque problème trouvé
donne lieu à une correction, avec un test qui le reproduit sur les serveurs de test.
