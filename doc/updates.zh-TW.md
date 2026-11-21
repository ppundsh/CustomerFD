# 安裝、版本檢測與自動更新

[English](updates.md)

CustomerFD 以單一安裝檔 `CustomerFD-Setup.exe` 發行，安裝於目前使用者，不需要系統管理員權限。控制中心會向 GitHub Releases 查詢新版本，在使用者同意後下載、**驗證數位簽章**、就地升級。流程與 cbxshell-rs 相同（同樣使用 minisign／Ed25519 簽章與 Tauri 格式的 `latest.json`）。

---

## 1. 安裝程式

| 項目 | 說明 |
|---|---|
| 安裝位置 | `%LOCALAPPDATA%\Programs\CustomerFD`（可用 `--dir` 或安裝畫面的「變更...」指定） |
| 權限 | `asInvoker`，只寫入 HKCU；不需要 UAC |
| 安裝內容 | `cfd-manager.exe`、`cfd_shell.dll`（x64，給檔案總管與 64 位元程式）、`x86\cfd_shell.dll`（給 32 位元程式的開啟／另存新檔視窗）、`uninstall.exe`（即安裝程式本身）、`LICENSE.txt`、`README.md` |
| 解除安裝 | 註冊於「設定 › 應用程式 › 已安裝的應用程式」（`HKCU\…\Uninstall\CustomerFD`：`DisplayVersion`、`UninstallString`、`QuietUninstallString`） |
| 開始功能表 | 「CustomerFD」捷徑（指向 `cfd-manager.exe`） |
| 側邊欄註冊 | 安裝程式直接呼叫 `cfd_shell.dll` 的 `DllRegisterServer`（依現有 `config.json` 寫入 HKCU），不使用 `regsvr32` |

命令列參數：

| 參數 | 作用 |
|---|---|
| （無） | 圖形化安裝畫面 |
| `--quiet` / `--silent` / `/s` | 以預設選項靜默安裝（建立捷徑、加入側邊欄） |
| `--dir <資料夾>` | 指定安裝資料夾 |
| `--uninstall [--quiet] [--purge]` | 解除安裝；`--purge` 會一併刪除 `%LOCALAPPDATA%\CustomerFD`（設定與自訂圖示）。互動模式會詢問是否刪除 |
| `--update --dir <資料夾> [--wait-pid <PID>]` | **就地更新**：等待控制中心結束、替換程式檔、只有在原本已加入側邊欄時才重新註冊、更新 `DisplayVersion`，**保留所有設定**；完成後以 `--updated` 重新開啟控制中心。由控制中心的更新器呼叫 |
| `--check [--quiet]` | 檢查執行環境；已安裝時一併對已安裝的版本做自我測試，並顯示結果（加 `--quiet` 只回傳結束代碼） |
| `--lang zh-TW` / `--lang en` | 安裝程式語言（預設跟隨 `config.json`） |

靜默模式的結束代碼：`0` 成功、`1` 安裝失敗、`2` 這部電腦無法執行 CustomerFD、`3` 已安裝但自我測試失敗。

**環境檢查與自我測試**：安裝前確認是 x64 的 Windows 10 1803（組建 17134）以上，而且不是 ARM64 版 Windows（ARM64 的檔案總管無法載入 x64 擴充套件）；不符合時列為「無法安裝」並停用安裝按鈕。另外會提示但不阻擋：Windows 10（背景改用 Acrylic）、沒有檔案總管工作列（遠端應用程式、Server Core、替代殼層）、以系統管理員身分執行（側邊欄可能裝到另一個管理員帳號）。安裝或更新完成後，自我測試會確認每個側邊欄項目都註冊到剛安裝的 `cfd_shell.dll`，並像檔案總管一樣透過 COM 實際建立每個項目；失敗時會提示，但不會還原安裝。

被檔案總管載入的 `cfd_shell.dll` 無法覆寫，安裝程式會先把它改名為 `cfd_shell.dll.N.old` 再寫入新檔（Windows 允許改名使用中的 DLL）；下次安裝或更新時清除。檔案總管要重新啟動後才會載入新版擴充套件，控制中心更新完成時會提示這點。解除安裝時，仍被載入的 DLL 會移到 `%TEMP%`，安裝資料夾則由背景的 `cmd` 在安裝程式結束後刪除。

---

## 2. 版本檢測

- 控制中心「關於」頁的 **軟體更新** 卡片顯示目前版本、上次檢查日期與狀態，提供「檢查更新」「下載並安裝」「版本資訊」，以及「自動檢查更新」開關。導覽列的版本文字旁也會顯示「有新版本」。
- **自動檢查更新**（預設開啟，`config.json` 的 `auto_check_updates`）：控制中心啟動時，若距離上次成功檢查（`last_update_check`，Unix 秒）超過 24 小時，就在背景檢查一次。只會提示，**不會自動下載或安裝**。
- 版本比較採 `主.次.修訂`；同版號時正式版高於預覽版（`0.3.1` > `0.3.1-beta.1`）；無法解析的版本一律不視為新版。

清單位址（永遠指向最新的正式版）：

```
https://github.com/ppundsh/CustomerFD/releases/latest/download/latest.json
```

`latest.json` 格式：

```json
{
  "version": "0.3.1",
  "notes": "版本說明",
  "pub_date": "2026-12-01T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<CustomerFD-Setup.exe.sig 的內容>",
      "url": "https://github.com/ppundsh/CustomerFD/releases/download/v0.3.1/CustomerFD-Setup.exe"
    }
  }
}
```

---

## 3. 自動更新流程

1. 使用者按「下載並安裝」，在確認對話框按「下載並安裝」。
2. 以 WinHTTP 下載安裝檔（使用系統 Proxy，TLS 由 Windows 驗證；跟隨 GitHub 轉址到 CDN），顯示下載進度。
3. **驗證簽章**：以編譯進控制中心的公鑰（`assets/updater.pub`）驗證 minisign（Ed25519）簽章。驗證失敗即中止，不寫入、不執行任何檔案。
4. 寫入 `%TEMP%\CustomerFD-Update\CustomerFD-Setup-<版本>.exe`，以 `--update --dir <安裝資料夾> --wait-pid <控制中心 PID>` 啟動，接著關閉控制中心。
5. 安裝程式等待控制中心結束、替換檔案、重新開啟控制中心。失敗時顯示錯誤並開啟原本的版本。

### 安全設計

| 威脅 | 對策 |
|---|---|
| 竄改的安裝檔／中間人 | 下載內容必須通過內建公鑰的 Ed25519 簽章驗證；TLS 只是第二層 |
| 惡意清單指向其他網站 | `url` 必須以 `https://github.com/ppundsh/CustomerFD/releases/download/` 開頭，且不得含 `..`、`?`、`#`、`@`、反斜線或空白 |
| 降版攻擊 | 只接受版本號嚴格大於目前版本的清單 |
| 資源耗盡 | 清單 ≤ 64 KiB、安裝檔 ≤ 256 MiB；JSON 巢狀深度 ≤ 16 |
| 未設定金鑰的建置 | `assets/updater.pub` 沒有金鑰時，更新功能停用並提示改從 GitHub 下載；CI 也會拒絕發行這種版本 |
| 未經同意的安裝 | 自動檢查只會提示；下載與安裝都需要使用者按鈕並確認 |
| GitHub 帳號、CI 或簽章私鑰外洩 | 另需維護者 YubiKey 對版本與安裝檔 SHA-256 的核准簽章，見第 4 節 |

---

## 4. YubiKey 發行核准（第二道簽章）

只靠 CI 的 minisign 私鑰時，GitHub 帳號、Actions 工作流程或 Secrets 任何一處被入侵，攻擊者都能推送「簽章正確」的惡意更新。因此從 0.3.2 起，更新器另外要求**維護者用 YubiKey 簽的核准**：

| 層級 | 誰持有 | 證明什麼 |
|---|---|---|
| minisign 簽章（`.sig`） | GitHub `release` Environment 的 Secret | 安裝檔是 CI 建置出來的 |
| YubiKey 核准（`latest.json` 的 `approval`） | 維護者的 YubiKey（私鑰無法匯出） | 維護者親自確認了**這個版本號＋這個安裝檔 SHA-256** |

- 核准內容是固定格式的文字：`customerfd-release\nversion=<版本>\nsha256=<安裝檔 SHA-256>\n`。第一行是產品名稱，所以同一把 YubiKey 金鑰為其他專案（例如 cbxshell-rs）簽的核准，在這裡一律無效。
- 更新器自行計算下載內容的 SHA-256 重組這段文字，用 `assets/release-approvers.pub` 裡的公鑰驗證。只要該檔有任何一把金鑰，從該原始碼建置的版本就**一定要**核准；沒有核准或核准無效時中止更新。
- 標籤也必須用同一把 YubiKey 簽章（`git tag -s`）；`.githooks/pre-push` 會擋下未簽章的提交與標籤，CI 也會在建置前驗證標籤簽章。

### 兩種 YubiKey 金鑰都可以

| | GnuPG（OpenPGP 卡片，目前使用） | FIDO2 SSH 金鑰 |
|---|---|---|
| 適用 | 已用 `gpg --edit-key` › `keytocard` 把 Ed25519 金鑰放進 YubiKey | 用 `ssh-keygen -t ed25519-sk` 建立 |
| 設定腳本 | `scripts\setup-gpg-signing.ps1` | `scripts\setup-yubikey.ps1` |
| 核准簽章 | `gpg --detach-sign --armor`（SHA-256／SHA-512） | `ssh-keygen -Y sign -n customerfd-release` |
| `release-approvers.pub` | `openpgp ssh-ed25519 AAAA… openpgp:0x…`（`gpg --export-ssh-key <KEYID>!` 的輸出，只有公開金鑰值，不含 email） | `sk-ssh-ed25519@openssh.com AAAA…` |
| 觸碰證明 | 簽章本身無法證明觸碰；請用 `ykman openpgp keys set-touch sig on` 讓 YubiKey 每次簽章都要觸碰 | 簽章內含 UP／UV 旗標，更新器會檢查 |

更新器依 `approval` 的格式（`BEGIN PGP SIGNATURE` 或 `BEGIN SSH SIGNATURE`）選擇驗證方式；兩者都只接受 Ed25519，且都必須是 `release-approvers.pub` 列出的金鑰。

---

## 5. 發行者設定（一次性）

1. **YubiKey**：插上 YubiKey，執行 `scripts\setup-gpg-signing.ps1 -KeyId <KEYID>`。它不會建立或修改金鑰，只會設定這個 repo 的 `gpg.format=openpgp`、`gpg.program`、`user.signingkey`、commit／tag 一律簽章與 `pre-push` hook（`core.hooksPath=.githooks`），確認公開金鑰值已在 `assets/release-approvers.pub`，最後實際簽一次測試。使用 FIDO2 SSH 金鑰時改用 `scripts\setup-yubikey.ps1`。git 的 `user.email` 必須是金鑰使用者 ID 中、且已在 GitHub 驗證的 email，GitHub 才會顯示 Verified。
2. **minisign 金鑰**（CI 用，建議設定密碼）：

   ```powershell
   npx @tauri-apps/cli@2 signer generate -w "$HOME\.tauri\customerfd.key"
   ```

   把 `customerfd.key.pub` 的內容貼到 `assets/updater.pub` 並提交。**私鑰不可提交**，請另外備份。
3. **GitHub 儲存庫設定**：
   - **Settings › Environments › `release`**：勾選 **Required reviewers**（自己），「Deployment branches and tags」限定 `v*` 標籤，Secrets 放 `CFD_UPDATER_KEY`（私鑰檔案的**內容**）與 `CFD_UPDATER_KEY_PASSWORD`（沒有密碼就留空字串）。同名的 Repository secrets 要刪掉，否則不受 Environment 保護。
   - **Settings › Rules › Rulesets**：對 `refs/tags/v*` 限制只有管理員能建立／刪除／更新標籤；對預設分支開啟 **Require signed commits** 與禁止 force push。
   - 帳號 **Settings › Password and authentication**：把 YubiKey 註冊為 Security key／Passkey，並移除簡訊等較弱的 2FA 方式。

> 公鑰（minisign 或 YubiKey）一旦隨版本發佈就不要直接更換：舊版控制中心只認得舊公鑰。必須更換時，先發行一個「同時列出新舊金鑰、仍以舊金鑰簽署與核准」的過渡版本。

---

## 6. 發行步驟

發行流程依維護者的共用發行標準：專案專屬的值（repo、安裝檔名、核准命名空間、版本檔）只放在根目錄的 `release.config.psd1`，`release-manifest.ps1`、`scripts\approve-release.ps1` 與 `release.yml` 都從那裡讀取。

1. 插上 YubiKey，工作區乾淨、`.\run-checks.ps1` 通過後執行 `.\bump-version.ps1 patch -Push`：更新版本 → 簽章提交 → 簽章標籤 `vX.Y.Z`（並 `git verify-tag`）→ 推送分支與標籤（各需 YubiKey PIN／觸碰）。
2. GitHub Actions 等你在網頁上 **Review deployments › Approve** 後，依序：驗證標籤是核准者簽的 → 標籤與版本一致 → `assets/updater.pub` 有公鑰 → 測試 → `build.ps1` → `release-manifest.ps1`（minisign 簽署＋`latest.json`）→ 建立**草稿** Release。草稿不會出現在 `releases/latest`，使用者收不到。
3. 本機執行 `scripts\approve-release.ps1 -Tag vX.Y.Z`（需要 GitHub CLI：`winget install GitHub.cli`、`gh auth login`）：確認標籤簽章、下載草稿、核對版本與網址、顯示安裝檔 SHA-256 供確認，用 YubiKey 簽核准並在本機驗證、寫入 `latest.json`、上傳並發布。

> `releases/latest/download/latest.json` 只會指向**最新的已發布正式版**；草稿與預覽版（prerelease）不會推送給使用者。
>
> 0.3.1 以前的版本沒有內建核准金鑰，只檢查 minisign 簽章，所以仍能更新到 0.3.2；從 0.3.2 起，**每一次**發行都必須經過第 3 步，沒有核准的版本使用者裝不上去。
