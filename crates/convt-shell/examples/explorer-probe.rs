//! Run on Windows against the installed DLL, without opening Explorer.
#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    use std::ffi::{OsString, c_void};
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use windows::Win32::{
        System::{Com::*, LibraryLoader::*},
        UI::Shell::*,
    };
    use windows::core::*;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert!(args.len() >= 2, "explorer-probe <installed-DLL> <files...>");
    let paths: Vec<_> = args[1..].iter().map(PathBuf::from).collect();
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let module = LoadLibraryW(&HSTRING::from(&args[0]))?;
        let proc = GetProcAddress(module, s!("DllGetClassObject")).expect("COM export");
        let get: unsafe extern "system" fn(*const GUID, *const GUID, *mut *mut c_void) -> HRESULT =
            std::mem::transmute(proc);
        let mut raw = std::ptr::null_mut();
        get(
            &GUID::from_u128(0x710fb9a8_c47e_4b39_9cfa_e273ab1b78f8),
            &IClassFactory::IID,
            &mut raw,
        )
        .ok()?;
        let factory = IClassFactory::from_raw(raw);
        let root: IExplorerCommand = factory.CreateInstance(None)?;
        let mut pidls = Vec::new();
        for path in &paths {
            let mut pidl = std::ptr::null_mut();
            SHParseDisplayName(&HSTRING::from(path.as_os_str()), None, &mut pidl, 0, None)?;
            pidls.push(pidl);
        }
        let pointers: Vec<_> = pidls.iter().map(|p| *p as *const _).collect();
        let items: IShellItemArray = SHCreateShellItemArrayFromIDLists(&pointers)?;
        for pidl in pidls {
            CoTaskMemFree(Some(pidl.cast()));
        }
        // Enum after GetTitle as well as after GetState: selection capture
        // must not depend solely on the slow-state call order.
        let title = root.GetTitle(&items)?;
        println!("title={}", String::from_utf16_lossy(title.as_wide()));
        CoTaskMemFree(Some(title.0.cast()));
        let enumerate = || -> Result<Vec<String>> {
            let commands = root.EnumSubCommands()?;
            let mut targets = Vec::new();
            loop {
                let mut command = [None];
                let status = commands.Next(&mut command, None);
                if status == HRESULT(1) {
                    break;
                }
                status.ok()?;
                let command = command[0].take().expect("one command");
                let title = command.GetTitle(&items)?;
                targets.push(String::from_utf16_lossy(title.as_wide()).to_ascii_lowercase());
                CoTaskMemFree(Some(title.0.cast()));
            }
            Ok(targets)
        };
        let before_state = enumerate()?;
        let state = root.GetState(&items, true)?;
        let actual = enumerate()?;
        let cli = PathBuf::from(&args[0]).parent().unwrap().join("convt.exe");
        let mut lists = Vec::new();
        for path in &paths {
            let output = std::process::Command::new(&cli)
                .arg("targets")
                .arg(path)
                .arg("--menu")
                .output()
                .unwrap();
            let targets = if output.status.success() {
                String::from_utf8(output.stdout)
                    .unwrap()
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect()
            } else {
                vec![]
            };
            lists.push(targets);
        }
        let expected = convt_shell::common_targets(&lists);
        assert_eq!(actual, expected);
        assert_eq!(before_state, expected);
        assert_eq!(
            state,
            if expected.is_empty() {
                ECS_HIDDEN.0 as u32
            } else {
                ECS_ENABLED.0 as u32
            }
        );
        println!("PASS DLL targets: {:?} -> {:?}", paths, actual);
        // Optional capture fixture lives beside a COPY of the real DLL/CLI.
        // It proves Invoke passes UTF-16 paths safely without any GUI input.
        if std::env::var_os("CONVT_SHELL_CAPTURE").is_some() && !actual.is_empty() {
            let commands = root.EnumSubCommands()?;
            let mut command = [None];
            commands.Next(&mut command, None).ok()?;
            command[0].take().unwrap().Invoke(&items, None)?;
            println!("PASS Invoke dispatched {} paths", paths.len());
        }
        // Deliberately do not FreeLibrary: COM references still own DLL code.
        drop(OsString::from_wide(&[]));
        CoUninitialize();
    }
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Run explorer-probe on Windows");
}
