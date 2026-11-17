# CustomerFD

Windows 11 檔案總管側邊欄自訂資料夾擴充套件（Rust + Win32 Shell Namespace Extension），控制介面設計風格延續 `cbxshell-rs` 的 Win11 Fluent Dark/Light Mica 風格。

## 功能特色
- **Win11 檔案總管左側邊欄整合**：透過 `HKCU` Shell Namespace Extension（無需系統管理員權限）在檔案總管左側邊欄新增自訂根資料夾。
- **展開／摺疊與子資料夾路徑對應**：支援在側邊欄展開或摺疊自訂資料夾，並可為每個子資料夾設定任意實體或網路路徑（`IShellFolder` 委派至原生檔案系統）。
- **控制中心 (`cfd-manager.exe`)**：
  - 延續 `cbxshell-rs` 的 Win11 Mica 雙主題（自動跟隨系統深色／淺色模式與系統強調色）及反鋸齒渲染元件。
  - 自訂根資料夾名稱、圖示資源、是否隱藏桌面圖示。
  - 新增、編輯、刪除、排序（上移／下移）、啟用／停用子資料夾與瀏覽資料夾對話方塊。
  - 側邊欄即時預覽（可點擊展開／摺疊測試）、一鍵套用更新與重啟檔案總管。
  - 支援繁體中文 / English 切換。

## 專案架構
- `crates/cfd-core`：設定檔讀寫（`%LOCALAPPDATA%\CustomerFD\config.json`）、`HKCU` 登錄檔註冊／反註冊、`SHChangeNotify` 與圖示產生。
- `crates/cfd-shell`：`cfd_shell.dll` COM Shell Namespace Extension（`IShellFolder`, `IPersistFolder2`, `IEnumIDList`, `SHCreateShellFolderView`）。
- `crates/cfd-manager`：`cfd-manager.exe` Win32 Fluent 控制中心介面。

## 建置與執行
```powershell
# 建置 release 執行檔與 DLL（輸出至 dist\）
powershell -ExecutionPolicy Bypass -File .\build.ps1

# 建置、註冊並啟動控制中心
powershell -ExecutionPolicy Bypass -File .\deploy.ps1
```
