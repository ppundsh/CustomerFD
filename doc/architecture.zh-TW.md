# 架構說明

[English](architecture.md)

## 1. 元件

| Crate | 產出 | 職責 |
|---|---|---|
| `cfd-core` | rlib | 設定檔讀寫與舊版遷移、HKCU 註冊／反註冊、`SHChangeNotify`、重新啟動檔案總管、自訂圖示轉換 |
| `cfd-shell` | `cfd_shell.dll` | COM Shell Namespace Extension：`IShellFolder`、`IPersistFolder2`、`IEnumIDList`，由 explorer.exe 載入 |
| `cfd-ui` | rlib | Fluent 繪製工具（色票、Mica／Acrylic、DIB、控制項），控制中心與安裝程式共用 |
| `cfd-manager` | `cfd-manager.exe` | 控制中心，含版本檢查與自動更新（`update_core.rs` 純邏輯、`updater.rs` WinHTTP） |
| `cfd-installer` | `CustomerFD-Setup.exe` | 安裝、就地更新、解除安裝；內嵌上述程式檔 |

## 2. 側邊欄如何出現

每個根資料夾對應一個 CLSID，從 `{9F3B4A10-7C2E-4D8F-A512-6E8C3B9D1042}` 起連續保留 64 個（最後 4 碼 `1042`–`1081`）。註冊只寫 HKCU：

| 登錄機碼 | 內容 |
|---|---|
| `Software\Classes\CLSID\{clsid}` | 預設值＝名稱；`System.IsPinnedToNameSpaceTree`（顯示於側邊欄）；`SortOrderIndex` |
| `…\{clsid}\DefaultIcon` | 圖示位置（`.ico`，或 `dll,索引`） |
| `…\{clsid}\InprocServer32` | `cfd_shell.dll` 路徑、`ThreadingModel=Apartment` |
| `…\{clsid}\ShellFolder` | `Attributes`、`FolderValueFlags` |
| `Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\{clsid}` | 讓檔案總管列出這個項目 |
| `…\Explorer\HideDesktopIcons\NewStartPanel` 值 `{clsid}=1` | 「不要顯示在桌面」 |

套用時會先移除保留範圍內、目前設定沒有使用的 CLSID，再寫入所有根資料夾，最後送出 `SHChangeNotify`。

## 3. Shell 擴充

- `DllGetClassObject` 依 CLSID 在 `config.json` 找到對應的根資料夾，建立 `VirtualRootFolder`。
- 列舉時先列出該根資料夾已啟用的子資料夾（自訂 PIDL），若根資料夾設定了「內容來源資料夾」，再接著列出那個資料夾的內容（委派給系統的 `IShellFolder`）。
- 進入子資料夾後，所有操作委派給目標路徑的原生 `IShellFolder`，因此檔案操作、縮圖、右鍵選單都由 Windows 處理。
- DLL 每次需要時讀取 `config.json`，所以子資料夾的變更不必重新註冊就會生效；名稱、圖示、顯示設定則需要按「套用」寫入登錄檔。
- 安全性：所有匯出函式以 `catch_unwind` 包住，panic 不會進入 explorer.exe；`DllCanUnloadNow` 會計算存活的 COM 物件；鎖遭污染時仍可繼續。

## 4. 設定檔

`%LOCALAPPDATA%\CustomerFD\config.json`，自訂圖示放在同一資料夾的 `icons\`。

```json
{
  "show_in_sidebar": true,
  "hide_from_desktop": true,
  "ui_lang": "ZhTw",
  "root_folders": [
    { "id": "root_1", "clsid": "{9F3B4A10-…-6E8C3B9D1042}", "name": "自訂資料夾",
      "target_path": "", "icon": "shell32.dll,-4", "sort_order_index": 66 }
  ],
  "subfolders": [
    { "id": "sub_1", "root_id": "root_1", "name": "下載項目",
      "target_path": "C:\\Users\\…\\Downloads", "icon_path": "", "enabled": true }
  ],
  "auto_check_updates": true,
  "last_update_check": 0,
  "nav_collapsed": false
}
```

新欄位一律有預設值，舊版設定檔可直接讀取；v0.1.0 的單一根資料夾欄位（`root_name`、`root_icon`、`sort_order_index`）會在載入時轉換。

## 5. 控制中心

- 純 Win32／GDI 即時繪製，畫面先畫在 32 位元預乘 alpha 的 DIB 上再一次輸出；導覽列與標題列保持 alpha 0 透出 Mica。規範見 [ui-design.zh-TW.md](ui-design.zh-TW.md)。
- 每次繪製時登錄「點擊區域」（hit region）；動作在**放開滑鼠**時才執行，懸停／按下狀態由區域與動作比對得出。
- 開啟檔案或資料夾對話框時不持有狀態借用，對話框的訊息迴圈期間視窗仍可正常重繪（修正 v0.1.0 的重入問題）。
- 更新流程見 [updates.zh-TW.md](updates.zh-TW.md)。

## 診斷紀錄

殼層擴充套件可以把每次呼叫（含宿主程序）附加寫入 `%LOCALAPPDATA%\CustomerFD\shell-trace.log`，無法寫入時改寫 `%TEMP%\cfd-shell-trace.log`。開關是 `HKLM\Software\CustomerFD` 下的 DWORD `ShellTrace`，所以只有系統管理員能變更：控制中心（關於 › 診斷）會透過 UAC 以 `--set-shell-trace on|off` 重新啟動自己、等待結束後重新讀取設定。擴充套件每個程序只讀一次開關，因此變更要在重新啟動檔案總管後才生效。關閉時會刪除記錄檔；因為執行中的檔案總管在重新啟動前仍會繼續寫入，控制中心在開關為關閉時，於「重新啟動檔案總管」之後與啟動時都會再刪一次。「全部清理」與解除安裝都以目前使用者身分執行，不會移除這個值。
