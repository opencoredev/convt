# Windows Explorer menu

The MSI registers **Convert with Convt** in Explorer's classic menu (Windows 10, and **Show more options** on Windows 11). Each supported input extension gets a cascade under `Software\Classes\SystemFileAssociations\<ext>\shell\ConvertWithConvt`, with sub-verbs for the same common targets `convt targets --menu` prefers. Choosing a target runs:

```
convt-app.exe open --to <format> -- "%1"
```

The cascade uses `MultiSelectModel=Player` so one menu appears for a multi-file selection. Each sub-verb uses `Document` so Explorer starts the GUI exe once per file; the running app is single-instance and takes the later launches. `More options…` opens Quick convert without a target. A **Convt** shortcut in Send To is the fallback when a file type has no cascade.

Registration follows the install scope (`HKMU`: HKCU for the current per-user MSI). Uninstall deletes the cascade keys and the Send To shortcut. The verbs launch `convt-app.exe` (the GUI binary; PR #96 stops that process allocating a console). Helpers that spawn it should use `CREATE_NO_WINDOW`.

`crates/convt-shell` already implements an `IExplorerCommand` COM handler that probes `convt targets --menu` at click time. That is how the Windows 11 compact menu has to work. It is not registered by this installer.

## Windows 11 top-level menu (not in this installer)

Windows 11's compact menu only shows packaged `IExplorerCommand` handlers. Classic HKCU verbs never appear there. Shipping that layer needs:

1. A sparse MSIX that declares the existing `convt-shell` CLSID (`710fb9a8-c47e-4b39-9cfa-e273ab1b78f8`) and a logo. The DLL and `convt-app.exe` stay in `%LOCALAPPDATA%\Programs\convt`; the package is identity only.
2. A code-signing certificate whose subject matches the manifest `Publisher` (planned `CN=Convt`). Unsigned sparse packages cannot be registered. A self-signed cert works only after an administrator trusts it in `LocalMachine\TrustedPeople`.
3. MSI custom actions that register the signed package for the current user after the files are installed, unregister it before removal, and roll those actions back. Do not restart Explorer.
4. Keep the classic `SystemFileAssociations` verbs. They remain the Windows 10 and "Show more options" path, and they work without a certificate.

Do not register both a static cascade and an `ExplorerCommandHandler` on the same files: Explorer would show two Convert with Convt entries. The compact-menu package should own the Win11 top-level item only.

Build the COM consumer on Windows when iterating on the DLL:

```powershell
cargo build --release --target x86_64-pc-windows-msvc -p convt-shell --example explorer-probe
.\target\x86_64-pc-windows-msvc\release\examples\explorer-probe.exe `
  "$env:LOCALAPPDATA\Programs\convt\convt_shell.dll" C:\test\sample.png
```

A real Explorer right-click remains a separate visual check.

## Verify without a desktop session

`packaging/windows/smoke.ps1` checks the generated verb table, installs the MSI when one is present, asserts the HKCU verbs and Send To shortcut, invokes the PNG→JPEG verb command, then uninstalls and asserts the keys are gone.
