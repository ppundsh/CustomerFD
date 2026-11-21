# Where the sidebar items appear

[繁體中文](compatibility.zh-TW.md)

A CustomerFD root (for example "Dev") is a Shell Namespace Extension registered for the current user. Any place in Windows that shows folders through the shell namespace may list it, but each place has its own conditions. This is the inventory and how each case is handled.

## Supported

| Place | Conditions and notes |
|---|---|
| File Explorer navigation pane | The main purpose. Needs `System.IsPinnedToNameSpaceTree=1` (control center "Show in sidebar"). |
| Open / Save As / Select Folder dialogs of 64-bit programs | Since 0.3.3. The modern file dialogs (Common Item Dialog, and the Explorer-style `GetOpenFileName` / `GetSaveFileName`) only list navigation-pane items marked as containing file-system folders, so the roots and mapped folders carry `SFGAO_FILESYSANCESTOR` / `SFGAO_STORAGEANCESTOR` (registry `ShellFolder\Attributes = 0xB8800000`). The file pickers of Microsoft Store / WinUI apps run in a 64-bit broker and belong here too. |
| The same dialogs in 32-bit programs | Since 0.3.3. 32-bit programs read `HKCU\Software\Classes\WOW6432Node\CLSID` and can only load 32-bit DLLs, so Setup also installs `x86\cfd_shell.dll` and registers it under WOW6432Node. The control center (About › Installation) and `CustomerFD-Setup.exe --check` check this registration. |
| Programs run as administrator | Elevating the same account reads the same HKCU, so the items show. |
| The legacy "Browse for Folder" tree (`SHBrowseForFolder`) | The roots appear under "Desktop". When a program allows only file-system folders, a root itself cannot be chosen, the folders under it can. |

## Limits in use

| Case | Notes |
|---|---|
| A root itself is not a real path | Roots are virtual: you cannot save directly at the root level or pick the root as a folder, and "Copy as path" or "Open in Terminal" have no path there. Every folder below a root is a real file-system path and works normally. |
| "Hide from desktop" | Only meant for the desktop icon; with it on, some programs' folder trees may hide the root as well. |
| Address bar icon | First-level folders keep the previous location's icon in the address bar; see the known issues in the README. |

## Not supported (cannot be handled by design)

| Case | Why |
|---|---|
| Another account (including entering a different administrator account when elevating) | CustomerFD installs for the current account only (HKCU); other accounts install it themselves. |
| Programs that draw their own file dialogs (Java Swing, non-native Qt / GTK, some games and tools) | They do not use the Windows shell namespace and show no shell extension items at all (OneDrive and Dropbox neither). |
| Very old Windows 3.1-style file dialogs | They have no navigation pane. |
| Windows on ARM64 | File Explorer is an ARM64 process and cannot load the x64 extension; Setup blocks the install. |
| No Explorer shell (RemoteApp, Server Core, replacement shells) | There is no navigation pane to show; Setup warns. |

## How to check

- `CustomerFD-Setup.exe --check`: environment check plus a self-test of the installed version (64- and 32-bit registration).
- By hand: open the Open and Save As dialogs once in Notepad (64-bit) and once in any 32-bit program. Task Manager's Details tab shows a program's bitness with the "Platform" column.
