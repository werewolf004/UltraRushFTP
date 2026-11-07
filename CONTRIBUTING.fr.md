# Contribuer

[English](CONTRIBUTING.md) | **Français**

UltraRushFTP est un logiciel propriétaire dans un dépôt privé : les contributions sont les
bienvenues de la part des collaborateurs invités par le propriétaire, selon les termes de la
[licence](LICENSE.fr.md) (les contributions font partie du Logiciel).

## Mise en place

1. Windows 10 ou plus récent, [Rust](https://rustup.rs) (la version est choisie par
   `rust-toolchain.toml` : 1.99.1 avec `clippy`, `rustfmt` et la cible 32 bits),
   Visual Studio Build Tools (MSVC et SDK Windows), PowerShell 7.
2. Facultatif : `cargo install --locked cargo-deny` pour lancer les vérifications des
   dépendances en local.
3. Vérifier que tout fonctionne :

```text
cargo test --workspace --all-features
cargo run -p urftp-proto --example dev_server   # serveur FTP local, utilisateur test / secret
pwsh scripts/validate-server.ps1 -SelfTest      # scénario de validation sur deux serveurs locaux
cargo run -p urftp-gui
```

## Avant chaque commit

```text
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check                                  # si les dépendances ont changé
pwsh scripts/generate-dependencies.ps1            # si les dépendances ont changé
```

La CI lance les mêmes vérifications à chaque push et pull request ; une CI rouge bloque
la fusion.

## Règles

- **Langue** : code, commentaires, identifiants et messages de commit en anglais. Chaque
  texte visible par l'utilisateur passe par `urftp-i18n` (`t!("clé")`), avec la clé ajoutée
  dans **les deux** fichiers `locales/en.json` et `locales/fr.json` (les tests le
  vérifient).
- **Sûreté** : pas de `unsafe` en dehors de `urftp-sys` ; là, chaque bloc demande un
  commentaire `// SAFETY:` qui explique pourquoi il est correct.
- **Erreurs** : pas de `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` dans le code
  de production (refusés par Clippy). Renvoyer des erreurs typées (`thiserror`) ; les
  messages destinés à l'utilisateur sont typés pour que l'interface les traduise.
- **Tests** : chaque changement s'accompagne de tests unitaires, et de tests d'intégration
  contre les serveurs de test locaux (`crates/urftp-proto/tests/common`) pour le
  comportement des protocoles. Les tests n'accèdent jamais à Internet et ne parlent jamais
  à un FTPRush ouvert (utiliser `URUSHFTP_IPC_CLASS`).
- **Interface** : prendre les couleurs dans `theme::colors(ui)` (jamais une couleur en dur)
  et utiliser les aides de `theme.rs` — `icon_button` pour un bouton à symbole, `name` pour
  un contrôle sans texte visible, `labelled` / `grid_row` pour relier un libellé à son
  champ, `single_line` pour les champs de texte, `primary_button` / `danger_button`. Les
  tests du thème et de l'interface (`cargo test -p urftp-gui`) échouent sur un nom
  manquant, une cible de moins de 24 px ou un contraste sous WCAG 2.2 AA ; ajouter un test
  dans `app/ui_tests.rs` pour tout nouveau comportement clavier.
- **Documentation** : garder les documents anglais et français alignés (`*.md` /
  `*.fr.md`). Mettre à jour [docs/OPTIONS.fr.md](docs/OPTIONS.fr.md) pour chaque nouveau
  réglage, [CHANGELOG.fr.md](CHANGELOG.fr.md) sous *Non publié* (et la version anglaise),
  et les captures avec `pwsh scripts/screenshots.ps1` quand l'interface change.
- **Dépendances** : crates.io uniquement, licences permissives acceptées par `deny.toml` ;
  expliquer pourquoi une nouvelle dépendance est nécessaire dans la pull request et dans
  [docs/DEPENDENCIES.fr.md](docs/DEPENDENCIES.fr.md).

## Protection des données

Ne jamais commiter de vrais carnets de sites, exports, mots de passe, clés ou adresses de
serveurs. Le `.gitignore` exclut `thirdparty_source/`, `sites.json`, `settings.json`,
`stats.json`, `RushSite.xml`, `sitemanager.xml` et `*.ftp` ; vérifier tout de même
`git status` avant de commiter. Captures et exemples n'utilisent que des données de
démonstration (`127.0.0.1`, `example.com`).

## Méthode de travail

1. Avancer par petites étapes, chacune validée (compilation, tests, vérification manuelle)
   avant la suivante.
2. Un commit par étape, message en anglais, à l'impératif, première ligne de moins de 72
   caractères, détails dans le corps.
3. Pull request vers `main` décrivant ce qui change et comment c'est testé.

## Versions publiées

1. Déplacer les entrées *Unreleased* de `CHANGELOG.md` (et *Non publié* de
   `CHANGELOG.fr.md`) dans une nouvelle section `## [x.y.z] - AAAA-MM-JJ` ; mettre à jour
   les liens en bas.
2. Mettre `version = "x.y.z"` dans `[workspace.package]` du `Cargo.toml`.
3. Commiter, puis créer et pousser le tag : `git tag -a vx.y.z -m "UltraRushFTP x.y.z"` et
   `git push origin vx.y.z`. Le workflow *Release* vérifie la version, lance les tests,
   compile et publie le zip avec sa somme de contrôle et la section du journal des
   changements comme notes.
