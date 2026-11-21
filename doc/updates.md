# Installation, version check and auto-update

[繁體中文](updates.zh-TW.md)

CustomerFD ships as a single installer, `CustomerFD-Setup.exe`, installed for the current user without administrator rights. The control center checks GitHub Releases for new versions and, once you agree, downloads the installer, **verifies its digital signature** and upgrades in place. The design is the same as cbxshell-rs (minisign / Ed25519 signatures and a Tauri-style `latest.json`).

---

## 1. Installer

| Item | Details |
|---|---|
| Location | `%LOCALAPPDATA%\Programs\CustomerFD` (change with `--dir` or "Change..." in the installer) |
| Rights | `asInvoker`, writes to HKCU only; no UAC |
| Files | `cfd-manager.exe`, `cfd_shell.dll`, `uninstall.exe` (the installer itself), `LICENSE.txt`, `README.md` |
| Uninstall | Listed under Settings › Apps › Installed apps (`HKCU\…\Uninstall\CustomerFD`: `DisplayVersion`, `UninstallString`, `QuietUninstallString`) |
| Start menu | "CustomerFD" shortcut to `cfd-manager.exe` |
| Sidebar registration | The installer calls `DllRegisterServer` of `cfd_shell.dll` directly (writing HKCU from the existing `config.json`); no `regsvr32` |

Command line:

| Option | Effect |
|---|---|
| (none) | Graphical installer |
| `--quiet` / `--silent` / `/s` | Silent install with defaults (shortcut, add to sidebar) |
| `--dir <folder>` | Install folder |
| `--uninstall [--quiet] [--purge]` | Uninstall; `--purge` also deletes `%LOCALAPPDATA%\CustomerFD` (settings and custom icons). Interactive mode asks |
| `--update --dir <folder> [--wait-pid <PID>]` | **In-place update**: waits for the control center to exit, replaces the program files, re-registers only if the sidebar was registered before, updates `DisplayVersion`, **keeps all settings**, then reopens the control center with `--updated`. Used by the updater |
| `--check [--quiet]` | Environment check, plus a self-test of the installed version if there is one; shows the result (exit code only with `--quiet`) |
| `--lang zh-TW` / `--lang en` | Installer language (defaults to `config.json`) |

Exit codes of the quiet modes: `0` OK, `1` install failed, `2` this PC cannot run CustomerFD, `3` installed but the self-test failed.

**Environment check and self-test.** Before installing, Setup checks that this is x64 Windows 10 1803 (build 17134) or later and not Windows on ARM64 (whose File Explorer cannot load the x64 extension); those are blockers and disable the Install button. It also warns, without blocking, about Windows 10 (Acrylic instead of Mica), a missing Explorer taskbar (RemoteApp, Server Core, replacement shells) and running elevated (a different administrator account would get the sidebar instead of you). After installing or updating, a self-test checks that every sidebar root is registered to the installed `cfd_shell.dll` and instantiates each one through COM the way Explorer does; a failure is reported but does not undo the installation.

`cfd_shell.dll` cannot be overwritten while Explorer has it loaded, so the installer renames it to `cfd_shell.dll.N.old` first (Windows allows renaming a loaded DLL) and removes such files on the next install or update. Explorer uses the new extension after it restarts; the control center says so after an update. On uninstall a still-loaded DLL is moved to `%TEMP%`, and a hidden `cmd` removes the install folder after the uninstaller exits.

---

## 2. Version check

- The **Software update** card on the About page shows the current version, the last check date and the status, with "Check for updates", "Download and install", "Release notes" and an "automatic check" switch. The rail also shows "Update" next to the version.
- **Automatic check** (on by default, `auto_check_updates` in `config.json`): when the control center starts and the last successful check (`last_update_check`, Unix seconds) is more than 24 hours old, it checks once in the background. It only notifies — it **never downloads or installs by itself**.
- Versions compare as `major.minor.patch`; with the same numbers a release beats a pre-release (`0.3.1` > `0.3.1-beta.1`); unparsable versions are never treated as newer.

Manifest URL (always the newest published release):

```
https://github.com/ppundsh/CustomerFD/releases/latest/download/latest.json
```

`latest.json`:

```json
{
  "version": "0.3.1",
  "notes": "Release notes",
  "pub_date": "2026-12-01T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<contents of CustomerFD-Setup.exe.sig>",
      "url": "https://github.com/ppundsh/CustomerFD/releases/download/v0.3.1/CustomerFD-Setup.exe"
    }
  }
}
```

---

## 3. Update flow

1. The user clicks "Download and install" and confirms.
2. WinHTTP downloads the installer (system proxy, TLS validated by Windows, following GitHub's redirect to its CDN) with progress.
3. **Signature check** against the public key compiled into the control center (`assets/updater.pub`). On failure nothing is written or run.
4. The installer is written to `%TEMP%\CustomerFD-Update\CustomerFD-Setup-<version>.exe`, started with `--update --dir <install folder> --wait-pid <control center PID>`, and the control center closes.
5. The installer waits, replaces the files and reopens the control center. On failure it shows the error and reopens the previous version.

### Security

| Threat | Mitigation |
|---|---|
| Tampered installer / MITM | The download must pass the Ed25519 check with the built-in key; TLS is only a second layer |
| Manifest pointing elsewhere | `url` must start with `https://github.com/ppundsh/CustomerFD/releases/download/` and must not contain `..`, `?`, `#`, `@`, backslashes or spaces |
| Downgrade | Only strictly newer versions are accepted |
| Resource exhaustion | Manifest ≤ 64 KiB, installer ≤ 256 MiB, JSON depth ≤ 16 |
| Build without a key | With no key in `assets/updater.pub` updates are disabled (download from GitHub instead), and CI refuses to release such a build |
| Install without consent | The automatic check only notifies; download and install need a button press and a confirmation |
| Leaked GitHub account, CI or signing key | A maintainer YubiKey approval of the version and installer SHA-256 is also required, see section 4 |

---

## 4. YubiKey release approval (second signature)

With only the CI minisign key, a compromise of the GitHub account, the workflow or its secrets would be enough to ship a "correctly signed" malicious update. From 0.3.2 the updater therefore also requires **an approval signed by the maintainer's YubiKey**:

| Layer | Held by | Proves |
|---|---|---|
| minisign signature (`.sig`) | secret of the GitHub `release` environment | the installer was built by CI |
| YubiKey approval (`approval` in `latest.json`) | the maintainer's YubiKey (private key cannot be exported) | the maintainer personally approved **this version + this installer SHA-256** |

- The approval is a fixed-format text: `customerfd-release\nversion=<version>\nsha256=<installer SHA-256>\n`. Its first line names the product, so an approval made with the same YubiKey key for another project (such as cbxshell-rs) is never valid here.
- The updater rebuilds that text from the SHA-256 of what it downloaded and checks it against the keys in `assets/release-approvers.pub`. As soon as that file lists a key, every build from that source **requires** an approval; a missing or invalid one aborts the update.
- Release tags must be signed with the same YubiKey (`git tag -s`): `.githooks/pre-push` rejects unsigned commits and tags, and CI verifies the tag signature before building.

### Either kind of YubiKey key works

| | GnuPG (OpenPGP card, in use) | FIDO2 SSH key |
|---|---|---|
| For | an Ed25519 key moved onto the YubiKey with `gpg --edit-key` › `keytocard` | a key made with `ssh-keygen -t ed25519-sk` |
| Setup script | `scripts\setup-gpg-signing.ps1` | `scripts\setup-yubikey.ps1` |
| Approval | `gpg --detach-sign --armor` (SHA-256 / SHA-512) | `ssh-keygen -Y sign -n customerfd-release` |
| `release-approvers.pub` | `openpgp ssh-ed25519 AAAA… openpgp:0x…` (output of `gpg --export-ssh-key <KEYID>!`: key material only, no e-mail) | `sk-ssh-ed25519@openssh.com AAAA…` |
| Proof of touch | the signature cannot prove a touch; run `ykman openpgp keys set-touch sig on` so every signature needs one | the signature carries the UP / UV flags, which the updater checks |

The updater picks the check by the format of `approval` (`BEGIN PGP SIGNATURE` or `BEGIN SSH SIGNATURE`); both accept Ed25519 only, and only keys listed in `release-approvers.pub`.

---

## 5. One-time publisher setup

1. **YubiKey**: with the YubiKey plugged in, run `scripts\setup-gpg-signing.ps1 -KeyId <KEYID>`. It does not create or change keys; it configures this repo (`gpg.format=openpgp`, `gpg.program`, `user.signingkey`, always-sign commits and tags, the `pre-push` hook via `core.hooksPath=.githooks`), makes sure the key's public value is in `assets/release-approvers.pub` and signs a test. For a FIDO2 SSH key use `scripts\setup-yubikey.ps1` instead. git's `user.email` must be one of the key's user IDs and verified on GitHub for commits to show as Verified.
2. **minisign key** (for CI; a password is recommended):

   ```powershell
   npx @tauri-apps/cli@2 signer generate -w "$HOME\.tauri\customerfd.key"
   ```

   Paste the contents of `customerfd.key.pub` into `assets/updater.pub` and commit it. **Never commit the private key**; back it up separately.
3. **GitHub repository settings**:
   - **Settings › Environments › `release`**: enable **Required reviewers** (yourself), limit deployment tags to `v*`, and keep the secrets `CFD_UPDATER_KEY` (the **contents** of the private key file) and `CFD_UPDATER_KEY_PASSWORD` (empty string if none) here. Delete repository secrets of the same name: they are not protected by the environment.
   - **Settings › Rules › Rulesets**: only admins may create, delete or update `refs/tags/v*`; on the default branch require signed commits and block force pushes.
   - Account **Settings › Password and authentication**: register the YubiKey as a security key / passkey and remove weaker 2FA methods such as SMS.

> Do not swap a public key (minisign or YubiKey) once it has shipped: older control centers only trust the old one. If you must, first release a transition version that lists both keys and is still signed and approved with the old ones.

---

## 6. Releasing

Releases follow the maintainer's shared release standard: the project-specific values (repo, installer name, approval namespace, version file) live only in `release.config.psd1` at the repo root, which `release-manifest.ps1`, `scripts\approve-release.ps1` and `release.yml` read.

1. With the YubiKey plugged in, a clean work tree and `.\run-checks.ps1` passing, run `.\bump-version.ps1 patch -Push`: bump the version → signed commit → signed tag `vX.Y.Z` (checked with `git verify-tag`) → push branch and tag (each needs the YubiKey PIN / touch).
2. GitHub Actions waits for **Review deployments › Approve** on the web page, then: tag signed by an approver → tag matches the version → `assets/updater.pub` has a key → tests → `build.ps1` → `release-manifest.ps1` (minisign signature + `latest.json`) → **draft** release. A draft never appears under `releases/latest`, so users do not get it.
3. Run `scripts\approve-release.ps1 -Tag vX.Y.Z` locally (needs the GitHub CLI: `winget install GitHub.cli`, `gh auth login`): it checks the tag signature, downloads the draft, checks version and URL, shows the installer SHA-256 for you to confirm, signs the approval with the YubiKey and verifies it locally, writes it into `latest.json`, uploads it and publishes the release.

> `releases/latest/download/latest.json` only follows the **newest published release**; drafts and pre-releases never reach users.
>
> Versions up to 0.3.1 carry no approver key and only check the minisign signature, so they can still update to 0.3.2. From 0.3.2 on, **every** release must go through step 3; an unapproved release cannot be installed by users.
