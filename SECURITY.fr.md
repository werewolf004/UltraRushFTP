# Sécurité

[English](SECURITY.md) | **Français**

## Signaler une vulnérabilité

Le dépôt est privé : ses tickets ne sont visibles que par ses collaborateurs. Ouvrir un
ticket avec l'étiquette `security` (ou contacter directement le propriétaire,
`werewolf004`) en indiquant la version, les étapes pour reproduire et l'impact constaté ;
ne jamais y coller de vrais mots de passe, clés ou adresses de serveurs. Si le dépôt
devient public, le signalement privé de vulnérabilités de GitHub sera activé et devra être
utilisé à la place.
Une réponse est donnée sous 7 jours ; les correctifs sont publiés dans une version
corrective et listés dans la section *Sécurité* du
[journal des changements](CHANGELOG.fr.md).

| Version | Prise en charge |
|---|---|
| 0.1.x | ✅ |

## Modèle de sécurité

### Connexions
- **TLS** (FTPS) utilise rustls (TLS 1.2 et 1.3, sans OpenSSL). Par défaut, la chaîne de
  certificats est vérifiée avec le magasin de certificats de Windows et le nom d'hôte est
  contrôlé. Un certificat inconnu n'est jamais accepté en silence : son empreinte SHA-256
  est montrée à l'utilisateur, qui peut l'approuver ; elle est alors épinglée dans la fiche
  du site. « Accepter tout certificat » existe pour les serveurs de test, est signalé comme
  déconseillé, et `urftp` exige `--insecure` pour l'utiliser.
- Les **canaux de données** sont chiffrés par `PROT P` par défaut, et les sessions TLS sont
  reprises pour que les serveurs puissent vérifier que la connexion de données appartient
  bien à la connexion de contrôle.
- Les **clés d'hôte SFTP** ne sont approuvées que par empreinte SHA-256 ; une clé inconnue
  ou modifiée arrête la connexion et est montrée à l'utilisateur.
- Le **mode actif** n'accepte une connexion de données que depuis l'adresse du serveur.
- Les commandes contenant un saut de ligne sont refusées (pas d'injection de commande par
  les noms de fichiers) ; les mots de passe sont masqués dans tous les journaux
  (`PASS ****`).

### Données enregistrées
- Les mots de passe des sites et des proxys sont enregistrés sous la forme
  `dpapi:<base64>` dans `sites.json` / `settings.json` : DPAPI de Windows, lié au compte de
  l'utilisateur, avec une entropie propre à l'application. Copiés sur une autre machine ou
  un autre compte, les fichiers ne révèlent aucun mot de passe.
- Le dossier des données se trouve dans le profil de l'utilisateur
  (`%APPDATA%\UltraRushFTP`) et en hérite les permissions. En mode portable (`-f`),
  protéger soi-même le dossier choisi.
- Les imports ne déchiffrent jamais les mots de passe des autres clients : ceux de FTPRush
  v1/v2 et de FlashFXP ne sont pas importés du tout.

### Interfaces locales
- **Liaison mIRC** : comme FTPRush, UltraRushFTP crée une fenêtre cachée `TfmRush` et
  exécute les scripts qu'il reçoit (`RushScript`, `RushCommand`, `Invoke`). N'importe quel
  programme de la même session Windows peut les envoyer, exactement comme avec FTPRush. Ne
  pas faire tourner de logiciel douteux dans la session où UltraRushFTP est ouvert avec des
  sites sensibles.
- Le **serveur Ident** (désactivé par défaut) répond le nom configuré à tout hôte qui
  interroge le port 113 pendant qu'il tourne.

### Code et chaîne d'approvisionnement
- Le code `unsafe` est limité à la crate `urftp-sys` ; chaque bloc justifie pourquoi il est
  correct (imposé par Clippy). Toutes les autres crates utilisent
  `#![forbid(unsafe_code)]`.
- Aucun `unwrap`/`expect`/`panic`/`unreachable` dans le code de production ; les erreurs
  sont typées.
- Dépendances : crates.io uniquement, `Cargo.lock` versionné, compilations en `--locked`,
  `cargo deny` (licences, alertes RustSec, sources) à chaque push, Dependabot chaque
  semaine.
- Les actions GitHub sont épinglées sur des commits, le jeton de la CI est en lecture seule
  et les identifiants ne sont pas conservés après le checkout.

## Limites connues

- `RUSTSEC-2023-0071` (`rsa`, attaque temporelle « Marvin ») : aucune version corrigée
  n'existe. Elle concerne la signature avec une clé SSH **RSA** ; les clés ed25519 et
  ECDSA ne sont pas concernées. Préférer une clé ed25519 pour SFTP.
- Les binaires ne sont pas encore signés : vérifier le fichier `.sha256` publié avec chaque
  version.
- Pas encore audité par un tiers, ni validé sur des serveurs réels.
