# Architecture

[繁體中文](architecture.zh-TW.md)

## 1. Components

| Crate | Output | Responsibility |
|---|---|---|
| `cfd-core` | rlib | Configuration file and legacy migration, HKCU (un)registration, `SHChangeNotify`, Explorer restart, custom icon conversion |
| `cfd-shell` | `cfd_shell.dll` | COM shell namespace extension: `IShellFolder`, `IPersistFolder2`, `IEnumIDList`; loaded by explorer.exe |
| `cfd-ui` | rlib | Fluent drawing toolkit (tokens, Mica / Acrylic, DIB, controls) shared by the control center and the installer |
| `cfd-manager` | `cfd-manager.exe` | Control center, including version check and auto-update (`update_core.rs` pure logic, `updater.rs` WinHTTP) |
| `cfd-installer` | `CustomerFD-Setup.exe` | Install, in-place update, uninstall; embeds the program files |

## 2. How the sidebar entries appear

Each root folder has its own CLSID; 64 consecutive CLSIDs are reserved starting at `{9F3B4A10-7C2E-4D8F-A512-6E8C3B9D1042}` (last group `…1042`–`…1081`). Registration writes HKCU only:

| Key | Contents |
|---|---|
| `Software\Classes\CLSID\{clsid}` | default = name; `System.IsPinnedToNameSpaceTree` (show in sidebar); `SortOrderIndex` |
| `…\{clsid}\DefaultIcon` | icon location (`.ico`, or `dll,index`) |
| `…\{clsid}\InprocServer32` | path of `cfd_shell.dll`, `ThreadingModel=Apartment` |
| `…\{clsid}\ShellFolder` | `Attributes`, `FolderValueFlags` |
| `Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\{clsid}` | makes Explorer list the entry |
| `…\Explorer\HideDesktopIcons\NewStartPanel` value `{clsid}=1` | "Hide from the desktop" |

Applying first removes every reserved CLSID the configuration no longer uses, then writes all root folders and sends `SHChangeNotify`.

## 3. Shell extension

- `DllGetClassObject` looks the CLSID up in `config.json` and creates a `VirtualRootFolder` for that root.
- Enumeration lists the root's enabled subfolders (custom PIDLs) and, when the root has a content folder, then the contents of that folder (delegated to the system `IShellFolder`).
- Inside a subfolder everything is delegated to the native `IShellFolder` of the target path, so file operations, thumbnails and context menus are handled by Windows.
- The DLL reads `config.json` whenever it needs it, so subfolder changes take effect without re-registration; name, icon and visibility need **Apply** to write the registry.
- Safety: every export is wrapped in `catch_unwind` so a panic never unwinds into explorer.exe; `DllCanUnloadNow` counts live COM objects; poisoned locks are recovered.

## 4. Configuration

`%LOCALAPPDATA%\CustomerFD\config.json`; custom icons live in `icons\` next to it. See the Chinese version for a sample. New fields always have defaults, so older files load unchanged; the v0.1.0 single-root fields (`root_name`, `root_icon`, `sort_order_index`) are migrated on load.

## 5. Control center

- Pure Win32 / GDI immediate-mode drawing into a 32-bit premultiplied DIB, blitted once; the rail and title bar keep alpha 0 so Mica shows through. See [ui-design.md](ui-design.md).
- Each frame registers hit regions; actions run on **mouse-up**, and hover / pressed states come from matching regions to actions.
- File and folder dialogs run without holding the state borrow, so the window keeps repainting during their modal loops (fixes the v0.1.0 re-entrancy crash).
- Update flow: [updates.md](updates.md).

## Diagnostic logging

The shell extension can append a trace of its calls (one line per call, with the host process) to `%LOCALAPPDATA%\CustomerFD\shell-trace.log`, falling back to `%TEMP%\cfd-shell-trace.log`. The switch is the DWORD `ShellTrace` under `HKLM\Software\CustomerFD`, so only an administrator can change it: the control center (About › Diagnostics) re-launches itself through UAC with `--set-shell-trace on|off`, waits for it and re-reads the value. The extension reads the switch once per process, so changes apply after Explorer restarts. Turning it off deletes the log; because a running Explorer keeps writing until it restarts, the control center deletes it again after Restart Explorer and at start-up while the switch is off. Clean-up and uninstall run per user and leave this value alone.
