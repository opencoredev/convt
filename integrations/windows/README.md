# Windows Explorer menu

The per-user MSI installs **Convert with convt** in Explorer. Windows 11's compact menu uses a sparse MSIX identity and the Rust `convt-shell` COM handler. Windows 10 and **Show more options** use the same handler through classic HKCU registry verbs. No administrator privileges are needed for those registry entries.

The DLL asks the installed `convt.exe targets <file> --menu` for targets. It keeps the first file's order and offers only targets shared by every selected file. Unsupported selections and folders have no menu. Probes run without a console, time out after two seconds, and cache each extension for 30 seconds. Installing or removing document support therefore refreshes the menu without restarting Explorer.

Choosing a target launches `convt-app.exe open --show-progress --to <format> -- <files...>`. The existing Activity window displays progress and the output goes next to the input. The Windows-only flag leaves Linux and Finder launches unchanged. The app and its conversion subprocesses do not open console windows.

## Build and install

Build the pinned Windows payload first with `packaging/windows/build.ps1`. Then run:

```powershell
# Classic menu only. The sparse package is built but cannot be registered unsigned.
.\packaging\windows\installer.ps1

# Both menus, using a certificate already in CurrentUser\My with subject CN=Convt.
.\packaging\windows\installer.ps1 -CertificateThumbprint <thumbprint>
```

The builder needs the Windows SDK (`makeappx.exe` and `signtool.exe`) and Rust. The sparse package contains registration metadata and a logo; the executable and DLL stay in `%LOCALAPPDATA%\Programs\convt`. MSI installation registers the signed package for the current user after installing its files. Uninstallation unregisters it before deleting the payload and removes the HKCU verbs and COM registration. Rollback actions undo registration changes. MSI never restarts Explorer.

A public release needs a trusted code-signing certificate whose subject matches the manifest's `Publisher="CN=Convt"`. Leo has no release certificate yet. A self-signed certificate is suitable only for verification. Windows package deployment requires trusting it in the machine store; run the import step from an administrator PowerShell:

```powershell
$cert = New-SelfSignedCertificate -Type Custom -Subject 'CN=Convt' `
  -KeyUsage DigitalSignature -CertStoreLocation Cert:\CurrentUser\My `
  -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3','2.5.29.19={text}')
Export-Certificate -Cert $cert -FilePath "$env:TEMP\convt-test.cer"
Import-Certificate -FilePath "$env:TEMP\convt-test.cer" `
  -CertStoreLocation Cert:\LocalMachine\TrustedPeople
.\packaging\windows\installer.ps1 -CertificateThumbprint $cert.Thumbprint
```

Do not commit certificates or private keys. Remove the test certificate from CurrentUser\My and LocalMachine\TrustedPeople after uninstalling the test build. Unsigned builds intentionally install only the classic menu.

## Verify without touching the desktop

Build the public COM consumer and run it against the installed DLL and real test files:

```powershell
cargo build --release --target x86_64-pc-windows-msvc -p convt-shell --example explorer-probe
.\target\x86_64-pc-windows-msvc\release\examples\explorer-probe.exe `
  "$env:LOCALAPPDATA\Programs\convt\convt_shell.dll" C:\test\sample.png
Get-AppxPackage -Name Convt.Desktop
Get-ItemProperty 'HKCU:\Software\Classes\*\shell\convt'
```

The consumer loads `DllGetClassObject`, constructs `IExplorerCommand`, enumerates before and after `GetState`, and compares the submenu with the installed CLI. Give it multiple files to check target intersection. After uninstalling, the package and both registry keys must be absent. A real Explorer right-click remains a separate visual check; never drive a desktop while its owner is active or a game is focused.
