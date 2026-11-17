# CLAUDE.md — CustomerFD

這份文件寫給在本專案工作的 AI 助手與協作者。修改程式前請先讀完。

## 專案概要

CustomerFD 是 Windows 11 檔案總管**左側邊欄自訂資料夾**擴充套件，以 Rust + Win32 撰寫，
只寫入 `HKCU`，不需要系統管理員權限。控制中心的 UI/UX 一律以 `cbxshell-rs` 為準
（`D:\Workspace\CBXShell-fix\cbxshell-rs`，規範見其 `doc/ui-design.zh-TW.md`，本專案的副本在 `doc/ui-design.zh-TW.md`）。

| Crate | 產出 | 說明 |
|---|---|---|
| `crates/cfd-core` | rlib | 設定檔（`%LOCALAPPDATA%\CustomerFD\config.json`）、HKCU 註冊／反註冊、`SHChangeNotify`、圖示 |
| `crates/cfd-shell` | `cfd_shell.dll` | COM Shell Namespace Extension（`IShellFolder`、`IPersistFolder2`、`IEnumIDList`），**由 explorer.exe 載入** |
| `crates/cfd-manager` | `cfd-manager.exe` | Win32 / GDI 自繪的 Fluent 控制中心 |
| `crates/cfd-installer`（規劃中） | `CustomerFD-Setup.exe` | 安裝、解除安裝、就地更新 |

## 建置與執行（Windows，PowerShell）

```powershell
.\build.ps1                 # cargo build --release --workspace，輸出到 dist\
.\deploy.ps1                # build + regsvr32 /s dist\cfd_shell.dll + 啟動控制中心
.\uninstall.ps1             # 清除所有 CLSID、NameSpace、HideDesktopIcons 與設定資料夾
cargo test --workspace      # 單元測試（update_core 等純邏輯模組）
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

- 只能在 Windows（MSVC toolchain）建置與執行；Linux 上只能做 `cargo check` 等級的檢查且多半會因 `windows` crate 而失敗，不要當成驗證。
- `cfd_shell.dll` 被 explorer.exe 載入後會被鎖住；重新建置前先用控制中心「重新啟動檔案總管」或 `uninstall.ps1`，否則複製會失敗。

## 不可違反的規則

### Shell 擴充（cfd-shell）
- **絕不讓 panic 跨越 FFI 邊界**：每個 COM 方法與 `DllGetClassObject` 等匯出函式都要 `catch_unwind` 或保證不 panic，失敗回傳 `HRESULT`。panic 會讓 explorer.exe 當掉。
- 不在 DLL 內做網路、長時間 I/O 或 UI；所有耗時工作放在 manager。
- CLSID 保留區段：`{9F3B4A10-7C2E-4D8F-A512-6E8C3B9D1042}` 起算 64 個（最後 4 碼 `1042`–`1081`）。新增根資料夾只能從此區段配發；`uninstall.ps1` 依賴這個範圍。
- 只寫 `HKCU`。不要加入任何需要 UAC 的路徑。

### 設定檔
- `AppConfig` 新增欄位一律加 `#[serde(default)]`，舊版 `config.json` 必須能讀；遷移邏輯放在 `AppConfig::load()`。
- 舊欄位以 `skip_serializing` 保留讀取，不要直接刪除。

### 控制中心 UI（cfd-manager）
規範來源：`doc/ui-design.zh-TW.md`（取自 cbxshell-rs）。重點：
- 所有尺寸以 96 DPI 設計，經 `s(px, dpi)`（= `scale_px`）換算；字級用 pt。不得寫死實際像素。
- 字型：`Segoe UI Variable Text`（找不到時退回 `Segoe UI`）；圖示用 `Segoe Fluent Icons` 字形，**不使用 emoji**。
- 顏色只能從 `theme()` 的 token 取得；不得在繪製程式內寫死色碼。新增 token 時深色、淺色都要給值。
- 輔色衍生（懸停 +14% 白、按下 Dark1、對比文字 YIQ ≥ 150）集中在 `theme.rs`。
- 互動不位移：按下只換色，懸停放大不超過 1 px；動畫 ≤ 220 ms、ease-out；閒置時 `KillTimer`。
- 導覽列（alpha = 0 的 Mica 區）上的文字必須用 `DrawThemeTextEx(DTT_COMPOSITED)`。
- 一張卡片只放一個功能；有子設定的卡片做成可展開卡片（Expander）。
- 不使用 `MessageBoxW`；確認／錯誤用自繪 Modal。
- 繪製一律雙緩衝（32-bit 預乘 BGRA DIB → 單次 `BitBlt`）。
- 狀態存取要能容忍重入（開檔對話框等模態迴圈期間）：用 `try_borrow_mut`／`try_lock`，失敗時只排定重繪。

### 多語系
- 每個使用者可見字串都要有 zh-TW 與 en 兩種版本（目前以 `tr("中文", "English")`；若遷移到 `locales/*.json`，兩個檔案的 key 必須一致）。
- 預設語言為繁體中文。文件以繁體中文為主。

### 版本與更新
- 版本唯一來源：根目錄 `Cargo.toml` 的 `[workspace.package] version`。程式內一律用 `env!("CARGO_PKG_VERSION")`；用 `bump-version.ps1` 改版號，不要手改多處。
- 更新器安全規則（移植自 cbxshell-rs，**不得放寬**）：
  - 下載的安裝檔必須通過內建公鑰（`assets/updater.pub`）的 minisign／Ed25519 簽章驗證，失敗即中止、不寫檔不執行。
  - 清單 `url` 必須以本專案 GitHub Releases 下載前綴開頭，並拒絕 `..`、`?`、`#`、`@`、反斜線、空白。
  - 只接受版本號嚴格大於目前版本（防降版）；清單 ≤ 64 KiB、安裝檔 ≤ 256 MiB、JSON 深度 ≤ 16。
  - 自動檢查只提示，**不自動下載或安裝**；下載與安裝都需使用者按鈕＋確認。
  - `updater.pub` 無金鑰的建置要停用更新功能。
- **私鑰（minisign `.key`、YubiKey 相關）絕不提交**；公鑰隨版本發佈後不可直接更換。

## 文件與授權
- 授權 GPL-3.0-or-later（移植自 cbxshell-rs 的程式碼也是 GPL）；版權標示 `Copyright (C) 2026 ppundsh`。
- `README.md` 為中英雙語單一檔案（English／繁體中文說明兩節），截圖放 `doc/images/`。介面改動影響外觀時要更新截圖。
- 技術文件成對維護：`doc/<名稱>.md`（英文）與 `doc/<名稱>.zh-TW.md`，修改一邊時同步另一邊。
- GitHub：`https://github.com/ppundsh/CustomerFD`；更新清單 `releases/latest/download/latest.json`。

## 程式碼風格
- Rust 2021，`cargo fmt` 預設設定；`unsafe` 區塊盡量小並註明理由。
- 純邏輯（版本比較、JSON、雜湊、設定遷移）與 Win32 分開，放在可在單元測試中執行的模組。
- 註解與 commit 訊息可用英文或繁體中文，同一檔案內保持一致。

## 參考
- 改版計劃：`doc/PLAN.zh-TW.md`
- cbxshell-rs 對應實作：`crates/cbx-manager/src/{main,render_aa,update_core,updater}.rs`、`crates/cbx-installer`、`build_res.rs`、`bump-version.ps1`、`release-manifest.ps1`、`.github/workflows/release.yml`
