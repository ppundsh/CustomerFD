# CustomerFD 改版計劃：UI/UX 對齊 cbxshell-rs ＋ 版本檢查與自動更新

> 狀態：0.2.0（UI 對齊）與 0.3.0（安裝、更新、發行、文件）已一併實作於 v0.3.0（2026-11-18）。待辦見第 4 節。決策見第 3 節。

## 0. 現況與差距

CustomerFD 目前 v0.1.0，三個 crate，`cfd-manager/src/main.rs` 單檔 2,876 行。已有 Mica、
DwmExtendFrame、`DrawThemeTextEx`、反鋸齒元件與雙主題，但和 cbxshell-rs 規範（`doc/ui-design.zh-TW.md`）有以下落差：

| 項目 | CustomerFD 現況 | cbxshell-rs 規範 |
|---|---|---|
| 字型 | `Segoe UI` | `Segoe UI Variable Text` |
| 圖示 | emoji（📁 📂 ℹ） | `Segoe Fluent Icons` 字形 |
| 視窗 | 1020 × 720，無最小尺寸，系統標題列 | 自訂標題列（`WM_NCCALCSIZE`／`WM_NCHITTEST`／`DwmDefWindowProc`），標題 14 pt，設最小尺寸 |
| 背景材質 | 先試 Mica Alt(4) 再 Mica(2) | Mica(2)；21H2 用 attr 1029；Win10 Acrylic；透明效果關閉時不透明後援 |
| 導覽列 | 固定寬 210 | 195／60 可收合（`U+E700` 切換鈕）、版本文字在切換鈕右側、40 寬圖示欄 |
| 導覽動畫 | 無 | 選取指示條 220 ms＋45% 擠壓、按下 140 ms、頁面 Drill-in 200 ms |
| 語言切換 | 切換按鈕 | 底部語言按鈕＋彈出選單（應用程式內 Acrylic、105 ms 展開） |
| 卡片 | 一般卡片 | 「一卡一功能」、Expander（52 高標題列、2 px 接縫、160–200 ms 展開） |
| 控制項 | 部分 | Checkbox 勾選描繪動畫、Toggle 35% 指數平滑、Win11 TextBox 狀態、Subtle 命令按鈕、捲軸規格 |
| 對話框 | 無確認對話框，危險操作直接執行、結果寫在狀態文字 | 自繪 Modal：暗幕 65%／35%、圓角 4、多層陰影 |
| 色票 | 大致相同，缺 `text_disabled`、`btn_disabled_*`、`caution_*`、`modal_backdrop`、`sel_unselected_*`；淺色 `status_warn` 不同 | 見規範 §3 |
| 主題變更 | `WM_SETTINGCHANGE` | 另加 `WM_DWMCOLORIZATIONCOLORCHANGED` |
| 狀態 | `thread_local RefCell` | 可重入（`try_borrow`／`try_lock`） |
| 版本資訊 | 無 PE 版本資源、無 manifest、無版本顯示 | `build_res.rs` 產生 VERSIONINFO＋manifest＋圖示，介面顯示版本 |
| 安裝／更新 | `deploy.ps1`（regsvr32） | 單一安裝檔、HKCU 解除安裝項、就地更新、簽章驗證 |

## 1. 階段規劃

### 階段 A — 基礎（約 0.5 天）
1. `git init`、`.gitignore`（`target/`、`dist/`、`*.key`）、首次提交現況，remote 設為 `https://github.com/ppundsh/CustomerFD.git`；`Cargo.toml` 加 `repository`、`authors`。
2. 新增 `LICENSE`（GPL-3.0 全文；本專案移植 cbxshell-rs 的 GPL 程式碼，必須維持 GPL-3.0-or-later），各 crate 主要原始檔加簡短版權標頭（`Copyright (C) 2026 ppundsh` ＋ SPDX `GPL-3.0-or-later`）。
3. 加入 `CLAUDE.md`（已完成）、`doc/ui-design.zh-TW.md`（已複製）。
4. 移植 `build_res.rs`：三個產出物都帶 VERSIONINFO（由 `CARGO_PKG_VERSION` 產生）、`app.manifest`（PerMonitorV2、Common Controls 6、asInvoker）、`app.ico`；exe 加 `/DEPENDENTLOADFLAG:0x800`。
5. 移植 `bump-version.ps1`，版本唯一來源為 `Cargo.toml`。
6. `run-checks.ps1`：fmt、clippy、test。

### 階段 B — UI/UX 對齊（約 3–4 天）
先重構再改外觀，避免在 2,876 行單檔上疊加：
1. **拆檔**：`main.rs` → `app.rs`（狀態、wndproc）、`layout.rs`、`widgets/`（button、toggle、checkbox、textbox、expander、flyout、modal、scrollbar）、`pages/`（sidebar、subfolders、about）、`anim.rs`。盡量從 cbxshell 的對應函式移植，保持同樣的數值與命名。
2. **主題**：補齊色票 token、輔色衍生函式（hover、contrast、tint）、`WM_DWMCOLORIZATIONCOLORCHANGED`。
3. **視窗外框**：自訂標題列、最小尺寸、Mica／21H2／Win10／不透明後援、透明效果開關偵測。
4. **字型與圖示**：Segoe UI Variable Text（後援 Segoe UI），emoji 全面換成 Fluent 字形（資料夾 `E8B7`、新增資料夾 `E8F4`、資訊 `E946`、地球 `E774` 等，實作時再逐一核對）。
5. **導覽列**：195／60 收合、指示條動畫、按下動畫、版本／狀態文字、底部語言彈出選單。
6. **頁面與卡片**：Drill-in；依「一卡一功能」重新分卡。初步分配：
   - 側邊欄設定：根資料夾清單｜名稱與目標路徑｜圖示（Expander：內建圖示、自訂圖示）｜顯示於側邊欄（Toggle）｜隱藏桌面圖示（Toggle）｜即時預覽｜套用與重啟檔案總管
   - 子資料夾管理：新增／編輯（Expander）｜清單（Checkbox 啟用、上移下移、編輯、開啟、刪除用 Subtle 按鈕）
   - 關於：版本與**軟體更新**卡片｜設定資料夾｜移除與清理（警示橫幅＋Modal 確認）
7. **控制項**：依規範 §8 逐一對齊；刪除、清理、重啟檔案總管改用 Modal 確認。
8. **重入安全**：`RefCell` 改 `try_borrow_mut`，模態迴圈期間只 `InvalidateRect`。
9. i18n 維持 `tr("中文", "English")`，不遷移 JSON。
10. 視窗預設 1040 × 760、最小 880 × 620；其餘尺寸依規範。
11. 「關於」頁加入版權宣告、授權（GPL-3.0-or-later，可開啟 LICENSE）、GitHub 連結與致謝 cbxshell-rs。

### 階段 C — 安裝程式（約 1.5 天）
新增 `crates/cfd-installer`（移植 `cbx-installer`）：
- 安裝到 `%LOCALAPPDATA%\Programs\CustomerFD`，HKCU 解除安裝項（`DisplayVersion`、`UninstallString`、`QuietUninstallString`）、開始功能表捷徑。
- 參數：`--quiet`、`--dir`、`--uninstall`、`--update --dir <資料夾> --wait-pid <PID>`、`--lang`。
- 註冊 DLL：直接載入並呼叫 `DllRegisterServer`（取代 `regsvr32`），再依 `config.json` 重新寫入 NameSpace。
- 被 explorer 鎖住的 `cfd_shell.dll` 改名為 `*.old` 後寫入新檔；下次更新清除。更新後送 `SHChangeNotify`，必要時提示重啟檔案總管。
- 解除安裝整合 `uninstall.ps1` 的清理邏輯（CLSID 區段、HideDesktopIcons、可選刪除設定）。
- `build.ps1` 改為產出 `dist\CustomerFD-Setup.exe`（內嵌 exe＋dll）。

### 階段 D — 版本檢查與自動更新（約 2 天）
移植 `update_core.rs`（純邏輯，含單元測試）與 `updater.rs`（WinHTTP），修改：
- `ALLOWED_URL_PREFIX` → `https://github.com/ppundsh/CustomerFD/releases/download/`
- 清單位址 → `https://github.com/ppundsh/CustomerFD/releases/latest/download/latest.json`
- 先只做 minisign；`Manifest.approval` 欄位與驗證介面保留，核准命名空間預定 `customerfd-release`，之後加 YubiKey 時不必改清單格式
- 設定：`config.json` 新增 `auto_check_updates`（預設 true）、`last_update_check`（Unix 秒）。
- 行為：啟動時距上次檢查 > 24 h 就在背景執行緒檢查；有新版只在導覽列版本文字旁與「關於」頁提示。
- 「軟體更新」卡片：目前版本、上次檢查日期、狀態；按鈕「檢查更新」「下載並安裝」「版本資訊」；Toggle「自動檢查更新」。
- 下載進度條 → 驗證簽章 → 寫入 `%TEMP%\CustomerFD-Update\` → 以 `--update --wait-pid` 啟動安裝程式 → 關閉控制中心。
- 依賴：`minisign-verify = "0.2"`。

### 階段 E — 發行流程（約 1 天，部分需你本人操作）
- `release-manifest.ps1`：minisign 簽署安裝檔、產生 `latest.json`。
- `.github/workflows/release.yml`：推送 `v*` 標籤 → 測試 → 建置 → 簽署 → 建立**草稿** Release。
- 你需要做：建立公開儲存庫 `ppundsh/CustomerFD`、產生 minisign 金鑰（`npx @tauri-apps/cli@2 signer generate`）、把公鑰放 `assets/updater.pub`、私鑰放 GitHub Secrets（建議 `release` Environment＋Required reviewers）。
- 選擇性：YubiKey 第二道核准（`approve-release.ps1`、`setup-yubikey.ps1`、`pre-push` hook），可沿用 cbxshell 的同一把 YubiKey。

### 階段 F — 文件、截圖與版權（約 0.5 天）
- `README.md`：和 cbxshell-rs 相同結構，單一檔案內含 English 與「繁體中文說明」兩節（頂端互相跳轉）。內容：功能、截圖、安裝、使用、建置、版本更新、文件索引、授權條款與致謝。
- 截圖（UI 對齊完成後拍）：`doc/images/control-center.zh-TW.png`、`control-center.en.png`（深色）、`control-center.light.png`（選用）、`explorer-sidebar.png`（檔案總管側邊欄展開實景）。可由我透過電腦操作在你的機器上截取，或由你提供；截圖前清掉含個人路徑或帳號名的資料夾名稱。
- 中英技術文件：`doc/ui-design.md`／`.zh-TW.md`（取自 cbxshell）、`doc/updates.md`／`.zh-TW.md`（改寫為 CustomerFD 版）、`doc/architecture.md`／`.zh-TW.md`（Shell 擴充架構、CLSID 區段、設定檔格式）。
- 版權宣告：`LICENSE`、README「授權條款」、關於頁、安裝程式畫面、PE 版本資源的 `LegalCopyright`，一律 `Copyright (C) 2026 ppundsh`。

### 階段 G — 驗證
- `cargo test`：版本比較、清單解析、URL 白名單、簽章驗證（含竄改案例）、設定遷移。
- 手動檢查表：深色／淺色、輔色切換、DPI 100/150/200%、多螢幕不同 DPI、透明效果關閉、導覽列收合、兩種語言、檔案總管側邊欄實際展開與子資料夾導覽。
- 更新端到端：發一個 `v0.2.0` 正式版 → 安裝 → 發 `v0.2.1` → 從控制中心更新，確認設定保留、DLL 被替換、`.old` 被清除；再測竄改安裝檔與降版清單會被拒絕。

## 2. 建議順序與里程碑

| 里程碑 | 內容 | 版本 |
|---|---|---|
| M1 | 階段 A＋B（UI 對齊完成） | 0.2.0（仍用 deploy.ps1） |
| M2 | 階段 C＋D＋E＋F（第一個可自動更新的正式版，含 README 與截圖） | 0.3.0 |
| M3 | 用 0.3.1 實測線上更新 | 0.3.1 |

更新功能要等使用者手上已經有「含更新器的版本」才會生效，所以 M2 是第一個需要手動安裝的版本。

## 3. 決策紀錄

| 項目 | 決定 | 來源 |
|---|---|---|
| GitHub 儲存庫 | `https://github.com/ppundsh/CustomerFD`（公開） | 使用者指定 |
| README | 中英雙語、含截圖 | 使用者指定 |
| 版權宣告 | GPL-3.0-or-later，`Copyright (C) 2026 ppundsh` | 使用者要求版權宣告；持有人名稱為預設，可改 |
| 更新簽章 | 先只做 minisign，保留 YubiKey 擴充點 | 預設 |
| 視窗尺寸 | 1040 × 760，最小 880 × 620 | 預設 |
| 多語系 | 維持 `tr()` | 預設 |
| 安裝程式 | 需要（就地更新依賴它） | 預設 |

## 4. 實作後待辦

- [ ] 在 Windows 上執行 `run-checks.ps1`（Linux 只做過 Windows 目標的型別檢查與 clippy）。
- [ ] 產生 minisign 金鑰、把公鑰放進 `assets/updater.pub`、設定 GitHub `release` Environment 與 Secrets（見 `doc/updates.zh-TW.md` 第 4 節）。
- [ ] 截圖：`doc/images/control-center.zh-TW.png`、`control-center.en.png`、`explorer-sidebar.png`。
- [ ] 建立 GitHub 儲存庫並推送，發行 v0.3.0；再以 v0.3.1 實測線上更新（含竄改安裝檔、降版清單應被拒絕）。

## 5. 已知問題（擱置）

### 第一層資料夾的位置列 icon 維持上一個位置

- 現象：在根節點底下第一層的真實資料夾（例如「開發」底下的 CBXShell-fix），位置列路徑正確，但 icon 維持上一個位置（例如 Dropbox，或從「開發」雙擊進入時維持「開發」的 icon）。從側邊欄點或從檔案清單雙擊都一樣；第二層以下與根節點本身都正常。
- 已確認（`shell-trace` 紀錄）：根節點對這些項目回應的 `IShellIcon::GetIconOf`、`IExtractIconW`（解析為 `imageres.dll,-3`）、`GetDetailsEx`、`GetDisplayNameOf` 都正確；但導覽時位置列沒有向根節點要 icon。
- 已試過但無效：`IShellIcon`（4eaeef5，保留）、預先解析的 `IExtractIconW`（c03f315，保留）、根節點相對的 `ParseDisplayName`（62971fd，已撤回，從未被呼叫）。
- 不要再試：`GetDisplayNameOf(空 PIDL)` 改回報根節點的虛擬解析名稱（79f1aa6）。`SHGetNameFromIDList` 會回頭再問根節點同一件事，無限遞迴使 explorer.exe 崩潰，已撤回（7d8ba77）。
- 診斷方式：建立 `%LOCALAPPDATA%\CustomerFD\shell-trace.enabled` 後重啟檔案總管，紀錄寫在同目錄的 `shell-trace.log`（備援：`%TEMP%\cfd-shell-trace.log`、`%USERPROFILE%\AppData\LocalLow\CustomerFD\shell-trace.log`）。用完請刪除 `.enabled` 檔。
- 後續方向：只加紀錄、不改行為，找出位置列實際取得 icon 的介面（可能是根節點未實作、因此看不到呼叫的介面）。
