# UltraRushFTP

Client FTP / FTPS / SFTP / FXP moderne pour Windows, écrit en Rust.
Héritier spirituel d'UltraFXP et de FTPRush : transferts rapides, FXP site à site,
files d'attente, et pilotage depuis mIRC via une DLL compatible `rushmirc.dll`.

> Statut : moteur complet (FTP, FTPS, FXP, SFTP, proxys, Ident, file d'attente) ;
> première version de l'interface à onglets. Pas encore testé sur des serveurs réels.

## Objectifs

- **Protocoles** : FTP, FTPS (explicite AUTH TLS / implicite), SFTP (SSH), FXP site à site
  (y compris FXP sécurisé via `CPSV` / `SSCN`).
- **Interface graphique Windows** : gestionnaire de sites, panneaux distants côte à côte,
  file d'attente, journal, transferts multiples en parallèle.
- **Pilotage externe** : DLL compatible avec les scripts mIRC écrits pour `rushmirc.dll`
  (`Invoke`, `RushScript`, `RushCommand`, `LoadDll`, `UnloadDll`), plus une ligne de commande.
- **Rush** : connexions parallèles, reprise des transferts, filtres skiplist/priorités,
  limitation de bande passante.

## Compatibilité rushmirc.dll

Notes de protocole extraites de la source Delphi d'origine (non redistribuée dans ce dépôt) :

| Élément | Valeur |
|---|---|
| Classe de la fenêtre principale ciblée | `TfmRush` |
| Mémoire partagée app ↔ DLL | `FTPRush`, 4096 octets |
| Mémoire partagée DLL → mIRC | `mIRC`, 4096 octets, puis `WM_USER + 200` |
| `Invoke` | copie la requête dans `FTPRush`, envoie `WM_USER + 300` (`WM_EXCHANGE`), relit la réponse |
| `RushScript` | `WM_COPYDATA`, `dwData = 1000`, `wParam` = fenêtre de rappel si callback actif |
| `RushCommand` | `WM_COPYDATA`, `dwData = 1001` |
| Retour de commande brute | `WM_COPYDATA` vers la fenêtre de rappel, `wParam = 888`, `dwData` 1 = succès / 0 = échec ; lignes : site, commande, résultats |

Note : dans l'original, `EnableCallback` / `DisableCallback` sont définies mais absentes
des exports ; UltraRushFTP les exportera.

## Développement

```text
cargo run -p urftp-gui                               # interface
cargo run -p urftp-proto --example dev_server        # serveur FTP local de test (test / secret)
cargo test --workspace                               # tests
```

`URUSHFTP_CONNECT=ftp://test:secret@127.0.0.1:PORT` (ou `ftps://`, `ftpsi://`, `sftp://`)
remplit la connexion rapide et se connecte au démarrage.

## Prérequis de développement

- Rust stable, cible `x86_64-pc-windows-msvc` (la DLL mIRC devra aussi exister en
  `i686-pc-windows-msvc` pour les versions 32 bits de mIRC).
- Windows 10 ou plus récent.

## Licence

MIT — voir [LICENSE](LICENSE).
