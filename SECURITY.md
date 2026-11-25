# Security

**English** | [Français](SECURITY.fr.md)

## Reporting a vulnerability

Do not open a public issue. Use GitHub private vulnerability reporting
(*Security › Report a vulnerability* on the repository) with the version, the steps to
reproduce and the impact you observed; never paste real passwords, keys or server
addresses. GitHub secret scanning and push protection are enabled on the repository.
You will get an answer within 7 days; fixes are released as a patch version and listed in
the *Security* section of the [changelog](CHANGELOG.md).

| Version | Supported |
|---|---|
| 0.1.x | ✅ |

## Security model

### Connections
- **TLS** (FTPS) uses rustls (TLS 1.2 and 1.3, no OpenSSL). By default the certificate
  chain is verified against the Windows certificate store and the host name is checked.
  An unknown certificate is never accepted silently: the user is shown its SHA-256
  fingerprint and may trust it, which pins that fingerprint in the site record.
  "Accept any certificate" exists for test servers, is labelled as not recommended, and
  `urftp` requires `--insecure` for it. Besides ring's algorithms, ECDSA P-521
  certificates and RSA keys from 1024 bits are accepted (old FTP servers).
- **SFTP** offers russh's safe algorithms first; older ones (SHA-1 Diffie-Hellman,
  AES-CBC, 3DES, HMAC-SHA1, DSA keys) come last and are only used when the server offers
  nothing better.
- **Data channels** are encrypted with `PROT P` by default, and TLS sessions are reused
  so that servers can check the data connection belongs to the control connection.
- **SFTP host keys** are trusted by SHA-256 fingerprint only; an unknown or changed key
  stops the connection and is shown to the user.
- **Active mode** accepts a data connection only from the server address.
- Commands containing line breaks are refused (no command injection through file names);
  passwords are masked in every log (`PASS ****`).

### Data at rest
- Site and proxy passwords are stored as `dpapi:<base64>` in `sites.json` /
  `settings.json`: Windows DPAPI bound to the user account, with application-specific
  entropy. Copied to another machine or account, the files reveal no password.
- The data folder lives in the user profile (`%APPDATA%\UltraRushFTP`) and inherits its
  permissions. In portable mode (`-f`) protect the chosen folder yourself.
- Imports never decrypt other clients' passwords: FTPRush v1/v2 and FlashFXP passwords
  are not imported at all.

### Local interfaces
- **mIRC link**: like FTPRush, UltraRushFTP creates a hidden `TfmRush` window and runs the
  scripts it receives (`RushScript`, `RushCommand`, `Invoke`). Any program running in the
  same Windows session can send them, exactly as with FTPRush. Do not run untrusted
  software in the session where UltraRushFTP is open with sensitive sites.
- **Ident server** (off by default) answers the configured name to any host that queries
  port 113 while it runs.

### Code and supply chain
- `unsafe` code is confined to the `urftp-sys` crate; every block documents why it is
  sound (enforced by Clippy). All other crates `#![forbid(unsafe_code)]`.
- No `unwrap`/`expect`/`panic`/`unreachable` in production code; errors are typed.
- Dependencies: crates.io only, `Cargo.lock` committed, builds with `--locked`,
  `cargo deny` (licences, RustSec advisories, sources) on every push, Dependabot weekly.
- GitHub Actions are pinned to commit SHAs, the CI token is read-only and credentials are
  not persisted after checkout.

## Known limitations

- `RUSTSEC-2023-0071` (`rsa`, Marvin timing attack): no fixed version exists. It concerns
  signing with an **RSA** SSH key; ed25519 and ECDSA keys are not affected. Prefer an
  ed25519 key for SFTP.
- The binaries are not code-signed (no certificate is planned): check the `.sha256` file
  published with each release.
- Not yet audited by a third party, and not yet validated against real-world servers.
