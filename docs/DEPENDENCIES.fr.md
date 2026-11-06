# Dépendances

[English](DEPENDENCIES.md) | **Français**

UltraRushFTP dépend de **22 crates directement** et de **374 crates en tout** (Windows,
dépendances transitives comprises). La liste complète avec versions et licences est
générée par `scripts/generate-dependencies.ps1` dans
[dependencies-full.md](dependencies-full.md) ; la CI échoue si elle n'est pas à jour.

## Règles

- **Licences** : seules les licences permissives compatibles avec la distribution d'un
  binaire propriétaire sont acceptées (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib,
  CC0-1.0, Unicode-3.0, BSL-1.0, CDLA-Permissive-2.0), vérifiées par `cargo deny` à chaque
  push (`deny.toml`). Une exception limitée : les polices intégrées par egui
  (`epaint_default_fonts`, licences OFL-1.1 et Ubuntu Font), qui autorisent leur inclusion.
- **Mentions** : chaque version publiée contient `THIRD-PARTY-LICENSES.txt`, les textes de
  licence de toutes les dépendances d'exécution (`generate-dependencies.ps1 -Notices`).
- **Alertes de sécurité** : les alertes RustSec font échouer la compilation. Exceptions
  documentées dans `deny.toml` :
  - `RUSTSEC-2024-0436` — `paste` n'est plus maintenue ; macro de compilation d'`egui_dock`,
    aucun code à l'exécution.
  - `RUSTSEC-2023-0071` — `rsa` (attaque temporelle « Marvin »), utilisée par `russh` pour
    les clés SSH RSA ; aucune version corrigée n'existe. Seule la signature avec une clé RSA
    locale est concernée ; les clés ed25519 et ECDSA ne le sont pas. Conservée par décision
    du 2026-11-05 ; préférer les clés ed25519.
- **Sources** : crates.io uniquement (ni git ni autre registre).
- **Versions** : `Cargo.lock` est versionné et les compilations utilisent `--locked` ;
  Dependabot propose les mises à jour chaque semaine.

## Dépendances directes

| Crate | Version | Licence | Utilisée par | Rôle |
|---|---|---|---|---|
| `tokio` | 1.54 | MIT | proto, cli, gui | Entrées-sorties réseau asynchrones et minuteries |
| `rustls` | 0.23 | Apache-2.0 / ISC / MIT | proto | TLS 1.2/1.3 pour FTPS (fournisseur ring, sans OpenSSL) |
| `tokio-rustls` | 0.26 | MIT / Apache-2.0 | proto | rustls sur les flux tokio |
| `rustls-native-certs` | 0.8 | Apache-2.0 / ISC / MIT | proto | Magasin de certificats de Windows |
| `ring` | 0.17 | Apache-2.0 AND ISC | proto | Empreintes SHA-256, fournisseur cryptographique de rustls et russh |
| `russh` | 0.65 | Apache-2.0 | proto | Client SSH (transport de SFTP) |
| `russh-sftp` | 3.0 | Apache-2.0 | proto | Protocole SFTP |
| `thiserror` | 2.0 | MIT / Apache-2.0 | core, proto, script, sys, cli, gui | Erreurs typées |
| `parking_lot` | 0.12 | MIT / Apache-2.0 | proto, cli, gui | Verrous sans empoisonnement (pas d'`unwrap` sur les verrous) |
| `serde` | 1.0 | MIT / Apache-2.0 | core, gui | Sérialisation des sites et des options |
| `serde_json` | 1.0 | MIT / Apache-2.0 | core, gui, i18n | Fichiers JSON et catalogues |
| `regex` | 1.13 | MIT / Apache-2.0 | core | Masques en expressions régulières |
| `roxmltree` | 0.21 | MIT / Apache-2.0 | core | Analyseur XML en lecture seule pour les imports |
| `base64` | 0.23 | MIT / Apache-2.0 | core | Codage des secrets, imports |
| `windows-sys` | 0.61 | MIT / Apache-2.0 | sys | API Win32 (fenêtres, mémoire partagée, DPAPI) |
| `eframe` | 0.36 | MIT / Apache-2.0 | gui | Fenêtre native et rendu pour egui |
| `egui_extras` | 0.36 | MIT / Apache-2.0 | gui | Tableaux des panneaux de fichiers et de la file |
| `egui_dock` | 0.21 | MIT | gui | Panneaux réorganisables dans chaque onglet |
| `rfd` | 0.17 | MIT | gui | Dialogues de fichiers et boîtes de message natifs |
| `sys-locale` | 0.3 | MIT / Apache-2.0 | i18n | Langue de l'interface de Windows |
| `rand` | 0.10 | MIT / Apache-2.0 | proto (tests) | Données de test |
| `rcgen` | 0.14 | MIT / Apache-2.0 | proto (tests) | Certificats de test du serveur FTPS de test |
| `egui_kittest` | 0.36 | MIT / Apache-2.0 | gui (tests) | Tests de l'interface par l'arbre AccessKit (clavier, noms, taille des cibles) |

« / » signifie « ou » (la crate laisse le choix de la licence).

## Outillage

| Outil | Usage |
|---|---|
| Rust 1.99.1 (`rust-toolchain.toml`) avec `clippy`, `rustfmt` et la cible `i686-pc-windows-msvc` | Compilation, analyse, formatage |
| Visual Studio Build Tools (éditeur de liens MSVC, SDK Windows) | Édition des liens sous Windows |
| `cargo-deny` | Vérification des licences, alertes et sources |
| PowerShell 7 | Scripts de `scripts/` (exports de la DLL, journal des changements, dépendances, captures) |
| GitHub Actions : `actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `EmbarkStudios/cargo-deny-action`, `actions/upload-artifact` | CI et versions publiées, épinglées sur des commits |
