# Windows integration

Windows 11 shows third-party items in the new compact context menu only through an `IExplorerCommand` COM handler registered by a packaged app. Classic registry verbs (`HKCR\*\shell`) still work, but only under "Show more options".

## Plan

1. **Shell extension DLL** (`convt-shell`, Rust with the `windows` crate) that implements `IExplorerCommand` and `IEnumExplorerCommand`:
   - The root command is **Convert with convt**, with `ECF_HASSUBCOMMANDS`.
   - Subcommands come from `convt_engines::default_registry().targets(...)` for the selected files' extensions.
   - `Invoke` launches `convt-app.exe --convert --to <id> <files...>` and returns at once.
2. **Sparse MSIX package** that gives the unpackaged installer an identity, so Windows 11 loads the handler. It declares `windows.fileExplorerContextMenus` with an `ItemType` of `*` and points at the DLL's CLSID. The installer (MSI or Inno Setup) registers it with `Add-AppxPackage -ExternalLocation`.
3. **Classic fallback**: registry verbs under `HKCU\Software\Classes\*\shell\convt` with `ExtendedSubCommandsKey`, for Windows 10.
4. **Signing**: the sparse package and the DLL both need an Authenticode certificate that the package manifest's `Publisher` matches.

References: Microsoft's "Integrate packaged desktop apps with File Explorer" docs and the `PhotoStoreDemo` sparse package sample.
