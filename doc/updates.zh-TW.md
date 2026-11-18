# 安裝、版本檢測與自動更新

[English](updates.md)

CustomerFD 以單一安裝檔 `CustomerFD-Setup.exe` 發行，安裝於目前使用者，不需要系統管理員權限。控制中心會向 GitHub Releases 查詢新版本，在使用者同意後下載、**驗證數位簽章**、就地升級。流程與 cbxshell-rs 相同（同樣使用 minisign／Ed25519 簽章與 Tauri 格式的 `latest.json`）。

---

## 1. 安裝程式

| 項目 | 說明 |
|---|---|
| 安裝位置 | `%LOCALAPPDATA%\Programs\CustomerFD`（可用 `--dir` 或安裝畫面的「變更...」指定） |
| 權限 | `asInvoker`，只寫入 HKCU；不需要 UAC |
| 安裝內容 | `cfd-manager.exe`、`cfd_shell.dll`、`uninstall.exe`（即安裝程式本身）、`LICENSE.txt`、`README.md` |
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
| `--lang zh-TW` / `--lang en` | 安裝程式語言（預設跟隨 `config.json`） |

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
| GitHub 帳號或 CI 被入侵（選用防護） | 可再加上維護者 YubiKey 核准簽章，見第 6 節 |

---

## 4. 發行者設定（一次性）

1. **產生 minisign 金鑰**（建議設定密碼）：

   ```powershell
   npx @tauri-apps/cli@2 signer generate -w "$HOME\.tauri\customerfd.key"
   ```

2. 把 `customerfd.key.pub` 的內容貼到 `assets/updater.pub`（保留檔案開頭的 `#` 說明行也可以），並提交。**私鑰不可提交**，請另外備份。
3. **GitHub Secrets**：在 **Settings › Environments** 建立名為 `release` 的 Environment，勾選 **Required reviewers**（你自己），「Deployment branches and tags」限定 `v*` 標籤，再加入：
   - `CFD_UPDATER_KEY`：私鑰檔案的**內容**
   - `CFD_UPDATER_KEY_PASSWORD`：私鑰密碼（沒有就留空字串）
4. 建議在 **Settings › Rules › Rulesets** 對 `refs/tags/v*` 限制只有管理員能建立或刪除標籤，對預設分支禁止 force push。

> 公鑰一旦隨版本發佈就不要直接更換：舊版控制中心只認得舊公鑰。必須更換時，先發行一個仍以舊金鑰簽署、但內含新公鑰的過渡版本。

---

## 5. 發行步驟

1. `.\bump-version.ps1`（或 `.\bump-version.ps1 0.3.1 -Tag -Push`）更新 `Cargo.toml` 版本、提交並建立 `vX.Y.Z` 標籤。
2. 推送標籤後 GitHub Actions 依序：確認標籤與版本一致 → 確認 `assets/updater.pub` 有公鑰 → 測試 → `build.ps1` → `release-manifest.ps1`（minisign 簽署＋`latest.json`）→ 發布 Release，附上 `CustomerFD-Setup.exe`、`.sig`、`latest.json`、原始碼 zip。
3. 若 `release` Environment 設了 Required reviewers，需在 Actions 頁面按核准後才會取得私鑰。

本機手動發行時：

```powershell
.\build.ps1
$env:TAURI_SIGNING_PRIVATE_KEY = "$HOME\.tauri\customerfd.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<密碼>'
.\release-manifest.ps1 -Tag v0.3.1
# 上傳 dist\CustomerFD-Setup.exe、CustomerFD-Setup.exe.sig、latest.json 到同名 Release
```

> `releases/latest/download/latest.json` 只會指向**最新的已發布正式版**；草稿與預覽版（prerelease）不會推送給使用者。

---

## 6. 選用：YubiKey 發行核准（第二道簽章）

更新器已內建 cbxshell-rs 的 YubiKey 核准驗證，目前因 `assets/release-approvers.pub` 沒有金鑰而停用。啟用後，即使 GitHub 帳號、Actions 或 minisign 私鑰外洩，也無法推送更新：

- 把維護者的 `sk-ssh-ed25519@openssh.com` 公鑰（FIDO2，需 PIN＋觸碰）加入 `assets/release-approvers.pub`；只要有一把金鑰，從該原始碼建置的版本就**一定要**核准。
- 核准內容：`customerfd-release\nversion=<版本>\nsha256=<安裝檔 SHA-256>\n`，以 `ssh-keygen -Y sign -n customerfd-release` 簽署，放進 `latest.json` 的 `approval` 欄位。
- 發行流程需改成「CI 建立草稿 → 本機用 YubiKey 簽核准 → 發布」，可參考 cbxshell-rs 的 `scripts/approve-release.ps1` 與 `doc/updates.zh-TW.md` 第 4–6 節。
