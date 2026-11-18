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
| `--lang zh-TW` / `--lang en` | Installer language (defaults to `config.json`) |

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
| Compromised GitHub account / CI (optional) | Add a maintainer YubiKey approval, see section 6 |

---

## 4. One-time publisher setup

1. **Generate a minisign key** (a password is recommended):

   ```powershell
   npx @tauri-apps/cli@2 signer generate -w "$HOME\.tauri\customerfd.key"
   ```

2. Paste the contents of `customerfd.key.pub` into `assets/updater.pub` (the leading `#` lines may stay) and commit it. **Never commit the private key**; back it up separately.
3. **GitHub secrets**: under **Settings › Environments** create an environment named `release`, enable **Required reviewers** (yourself), limit deployment tags to `v*`, and add:
   - `CFD_UPDATER_KEY`: the **contents** of the private key file
   - `CFD_UPDATER_KEY_PASSWORD`: its password (empty string if none)
4. Recommended: a ruleset on `refs/tags/v*` so only admins can create or delete release tags, and no force pushes to the default branch.

> Do not swap the public key once it has shipped: older control centers only trust the old key. If you must, first release a transition version signed with the old key that contains the new key.

---

## 5. Releasing

1. `.\bump-version.ps1` (or `.\bump-version.ps1 0.3.1 -Tag -Push`) updates the version in `Cargo.toml`, commits and tags `vX.Y.Z`.
2. Pushing the tag runs GitHub Actions: tag matches version → `assets/updater.pub` has a key → tests → `build.ps1` → `release-manifest.ps1` (minisign signature + `latest.json`) → publish the release with `CustomerFD-Setup.exe`, `.sig`, `latest.json` and a source zip.
3. With required reviewers on the `release` environment, approve the run in the Actions tab before it receives the key.

Manual release from your PC:

```powershell
.\build.ps1
$env:TAURI_SIGNING_PRIVATE_KEY = "$HOME\.tauri\customerfd.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<password>'
.\release-manifest.ps1 -Tag v0.3.1
# upload dist\CustomerFD-Setup.exe, CustomerFD-Setup.exe.sig and latest.json to the release
```

> `releases/latest/download/latest.json` only follows the **newest published release**; drafts and pre-releases never reach users.

---

## 6. Optional: YubiKey release approval

The updater already contains cbxshell-rs's YubiKey approval check; it is inactive while `assets/release-approvers.pub` lists no key. Once enabled, a leaked GitHub account, workflow or minisign key is no longer enough to ship an update:

- Add the maintainer's `sk-ssh-ed25519@openssh.com` public key (FIDO2, PIN + touch) to `assets/release-approvers.pub`. As soon as one key is listed, every build from that source **requires** an approval.
- The approval signs `customerfd-release\nversion=<version>\nsha256=<installer SHA-256>\n` with `ssh-keygen -Y sign -n customerfd-release` and is stored in the `approval` field of `latest.json`.
- The release flow then becomes "CI creates a draft → approve locally with the YubiKey → publish"; see cbxshell-rs `scripts/approve-release.ps1` and its `doc/updates.md`, sections 4–6.
