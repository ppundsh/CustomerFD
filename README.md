# CustomerFD

Custom folders in the Windows 11 File Explorer sidebar — written in Rust, installed per user, no administrator rights.

[English](#english) · [繁體中文說明](#繁體中文說明)

---

<a name="english"></a>
## English

![CustomerFD Control Center](./doc/images/control-center.en.png)

![Custom folders in the File Explorer sidebar](./doc/images/explorer-sidebar.png)

### Features
- **Your own entries in the Explorer navigation pane.** Add up to 64 root folders to the left sidebar of File Explorer, each with its own name and icon. Expand, collapse and browse them like regular folders.
- **Any folder, anywhere, under one roof.** Map subfolders to any local or network path (`D:\Work`, `\\NAS\Share`) and reorder, enable or disable them; changes show up immediately. Optionally merge the contents of one folder into the root itself.
- **Per-user and clean.** Everything lives in `HKCU` and `%LOCALAPPDATA%`. No UAC prompt, no system-wide changes, and a one-click clean-up that removes every trace. (The only exception is the optional diagnostic logging switch under About › Diagnostics, which asks for administrator rights.)
- **Windows 11 Fluent control center.** Mica backdrop, light / dark mode and accent colour that follow the system, collapsible navigation, animated controls, Traditional Chinese and English.
- **Safe automatic updates.** The control center checks GitHub Releases at most once a day and only tells you. When you choose to install, the download must pass an Ed25519 (minisign) signature check before anything runs, and your settings are kept.

### Install
1. Download `CustomerFD-Setup.exe` from [Releases](https://github.com/ppundsh/CustomerFD/releases/latest).
2. Run it and click **Install**. It installs to `%LOCALAPPDATA%\Programs\CustomerFD`, adds a Start menu shortcut and puts a default custom folder in the sidebar.
3. In the control center, rename the root folder, pick an icon, add subfolders, then click **Apply**. If the sidebar does not change right away, click **Restart Explorer**.

Command-line options:

| Option | Effect |
|---|---|
| `--quiet` | Silent install with default options |
| `--dir <folder>` | Install folder |
| `--uninstall [--quiet] [--purge]` | Uninstall (`--purge` also deletes settings and custom icons) |
| `--lang zh-TW` / `--lang en` | Installer language |

Uninstall from **Settings › Apps › Installed apps › CustomerFD**.

### Requirements
Windows 11 (Windows 10 1803+ works with an Acrylic instead of a Mica backdrop), x64. Windows on ARM64 is not supported. Setup checks these before installing and self-tests the sidebar afterwards (`CustomerFD-Setup.exe --check` runs the check on its own).

### Build from source
Requires Rust (stable, MSVC toolchain) and the Windows 10/11 SDK (`rc.exe`, for icons and version info).

```powershell
.\build.ps1        # release build → dist\CustomerFD-Setup.exe
.\deploy.ps1       # build, install for the current user, open the control center
.\run-checks.ps1   # fmt, clippy, tests, build
```

### Known issues

- When you open a real folder directly under a sidebar root, the address bar shows the correct path but keeps the previous location's icon. Folders one level deeper show the right icon.

### Documentation
- 🏛️ [Architecture](./doc/architecture.md) — shell namespace extension, registry layout, configuration file.
- 🧭 [Where the sidebar items appear](./doc/compatibility.md) — Explorer, Open / Save As dialogs (64- and 32-bit), folder pickers, and the known limits.
- ⬆️ [Installation, version check and auto-update](./doc/updates.md) — installer switches, update flow, release signing.
- 🎨 [UI design guidelines](./doc/ui-design.md) — Fluent tokens and interaction rules shared with cbxshell-rs.

### License and acknowledgements
- Copyright © 2026 ppundsh. Distributed under the **GNU General Public License v3.0 or later** (`GPL-3.0-or-later`); see [LICENSE](./LICENSE).
- The control center's visual design, updater and installer are adapted from [cbxshell-rs](https://github.com/ppundsh/cbxshell-rs) (also GPL-3.0-or-later).

---

<a name="繁體中文說明"></a>
## 繁體中文說明

![CustomerFD 控制中心](./doc/images/control-center.zh-TW.png)

![檔案總管側邊欄中的自訂資料夾](./doc/images/explorer-sidebar.png)

### 功能特色
- **在檔案總管側邊欄加入自己的項目**：最多 64 個根資料夾，各自有名稱與圖示，可以像一般資料夾一樣展開、摺疊與瀏覽。
- **把散落各處的資料夾集中在一起**：子資料夾可以對應任何本機或網路路徑（`D:\Work`、`\\NAS\Share`），可排序、啟用或停用，變更立即生效；也可以讓根資料夾直接顯示某個資料夾的內容。
- **只影響目前使用者**：設定都在 `HKCU` 與 `%LOCALAPPDATA%`，不跳出 UAC、不修改系統範圍設定，並提供一鍵完全清理。（唯一例外是「關於 › 診斷」裡選用的診斷紀錄開關，切換時需要系統管理員權限。）
- **Windows 11 Fluent 控制中心**：Mica 背景材質，深色／淺色模式與輔色跟隨系統，可收合的導覽列與動畫控制項，支援繁體中文與英文。
- **安全的自動更新**：控制中心每天最多向 GitHub Releases 檢查一次，只會提示；你選擇安裝後，下載的安裝檔必須先通過 Ed25519（minisign）數位簽章驗證才會執行，所有設定都會保留。

### 安裝
1. 從 [Releases](https://github.com/ppundsh/CustomerFD/releases/latest) 下載 `CustomerFD-Setup.exe`。
2. 執行後按「安裝」。程式會安裝到 `%LOCALAPPDATA%\Programs\CustomerFD`，建立「開始」功能表捷徑，並在側邊欄加入「自訂資料夾」。
3. 在控制中心修改根資料夾名稱、選擇圖示、新增子資料夾，然後按「套用」。若側邊欄沒有立即更新，按「重新啟動檔案總管」。

命令列參數：

| 參數 | 作用 |
|---|---|
| `--quiet` | 以預設選項靜默安裝 |
| `--dir <資料夾>` | 指定安裝資料夾 |
| `--uninstall [--quiet] [--purge]` | 解除安裝（`--purge` 會一併刪除設定與自訂圖示） |
| `--lang zh-TW` / `--lang en` | 安裝程式語言 |

解除安裝：「設定 › 應用程式 › 已安裝的應用程式 › CustomerFD」。

### 系統需求
Windows 11（Windows 10 1803 以上也可使用，背景材質改為 Acrylic），x64。不支援 ARM64 版 Windows。安裝程式會在安裝前檢查這些條件，安裝後自我測試側邊欄（`CustomerFD-Setup.exe --check` 可單獨執行檢查）。

### 從原始碼建置
需要 Rust（stable，MSVC 工具鏈）與 Windows 10/11 SDK（`rc.exe`，用於圖示與版本資訊）。

```powershell
.\build.ps1        # release 建置 → dist\CustomerFD-Setup.exe
.\deploy.ps1       # 建置、為目前使用者安裝並開啟控制中心
.\run-checks.ps1   # fmt、clippy、測試、建置
```

### 已知問題

- 開啟側邊欄根節點底下第一層的真實資料夾時，位置列的路徑正確，但 icon 會維持上一個位置的圖示；再往下一層的資料夾則正常。

### 技術文件
- 🏛️ [架構說明](./doc/architecture.zh-TW.md)：Shell Namespace Extension、登錄檔配置、設定檔格式。
- 🧭 [側邊欄項目會出現在哪裡](./doc/compatibility.zh-TW.md)：檔案總管、開啟／另存新檔視窗（64 與 32 位元）、選擇資料夾視窗，以及已知限制。
- ⬆️ [安裝、版本檢測與自動更新](./doc/updates.zh-TW.md)：安裝參數、更新流程、發行簽章。
- 🎨 [UI 設計規範](./doc/ui-design.zh-TW.md)：與 cbxshell-rs 共用的 Fluent 色票與互動規則。

### 授權條款與致謝
- Copyright © 2026 ppundsh。本專案採用 **GNU General Public License v3.0 or later（`GPL-3.0-or-later`）** 授權釋出，詳見 [LICENSE](./LICENSE)。
- 控制中心的視覺設計、更新器與安裝程式改編自 [cbxshell-rs](https://github.com/ppundsh/cbxshell-rs)（同為 GPL-3.0-or-later）。
