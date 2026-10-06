use serde::Serialize;
use std::collections::HashMap;
use std::ffi::{OsString, c_void};
use std::io::Read;
use std::os::windows::{ffi::OsStringExt, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;
use windows::Win32::UI::Shell::*;
use windows::core::*;

pub const CLSID: GUID = GUID::from_u128(0x710fb9a8_c47e_4b39_9cfa_e273ab1b78f8);
static OBJECTS: AtomicUsize = AtomicUsize::new(0);
static SERVER_LOCKS: AtomicUsize = AtomicUsize::new(0);
struct ModuleLease;
impl ModuleLease {
    fn new() -> Self {
        OBJECTS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}
impl Drop for ModuleLease {
    fn drop(&mut self) {
        OBJECTS.fetch_sub(1, Ordering::SeqCst);
    }
}
const PROBE_LIMIT: Duration = Duration::from_secs(2);
const CACHE_TTL: Duration = Duration::from_secs(30);

fn error() -> Error {
    Error::from_hresult(E_FAIL)
}
fn text(value: &str) -> Result<PWSTR> {
    let wide = HSTRING::from(value);
    // Shell owns the CoTaskMem string returned by SHStrDupW.
    unsafe { SHStrDupW(&wide) }
}
fn install_dir() -> Result<PathBuf> {
    let mut module = HMODULE::default();
    let mut buffer = vec![0u16; 32768];
    // Resolve this DLL, not Explorer.exe/dllhost.exe and never PATH.
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(DllGetClassObject as *const () as *const u16),
            &mut module,
        )?;
        let len = GetModuleFileNameW(Some(module), &mut buffer) as usize;
        if len == 0 || len >= buffer.len() {
            return Err(error());
        }
        PathBuf::from(OsString::from_wide(&buffer[..len]))
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(error)
    }
}
fn files(items: Ref<IShellItemArray>) -> Result<Vec<PathBuf>> {
    let items = items.ok()?;
    let mut paths = Vec::new();
    unsafe {
        let count = items.GetCount()?;
        if count == 0 || count > 256 {
            return Err(error());
        }
        for index in 0..count {
            let item = items.GetItemAt(index)?;
            let name = item.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = PathBuf::from(OsString::from_wide(name.as_wide()));
            CoTaskMemFree(Some(name.0.cast()));
            if !path.is_absolute() || !path.is_file() {
                return Err(error());
            }
            paths.push(path);
        }
    }
    Ok(paths)
}

// Probe once per extension, not once per file. Cache expires so installing or
// removing document support updates the menu without restarting Explorer.
type Cache = HashMap<OsString, (Instant, Vec<String>)>;
static TARGETS: OnceLock<Mutex<Cache>> = OnceLock::new();
fn targets(path: &Path) -> Vec<String> {
    let Some(extension) = path.extension().map(|e| e.to_ascii_lowercase()) else {
        return vec![];
    };
    let cache = TARGETS.get_or_init(Default::default);
    if let Ok(cache) = cache.lock()
        && let Some((when, result)) = cache.get(&extension)
        && when.elapsed() < CACHE_TTL
    {
        return result.clone();
    }
    let Some(result) = probe(path) else {
        return vec![];
    };
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(extension, (Instant::now(), result.clone()));
    }
    result
}

#[derive(Serialize)]
struct ExplorerRequest<'a> {
    files: &'a [PathBuf],
    show_progress: bool,
    to: Option<&'a str>,
    preset: Option<&'a str>,
    source: &'static str,
    license: Option<&'static str>,
    auth: Option<()>,
}

fn request_dir() -> PathBuf {
    std::env::var_os("CONVT_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("convt"))
}

fn handoff(paths: &[PathBuf], target: &str) -> Result<()> {
    static REQUEST_ID: AtomicUsize = AtomicUsize::new(0);
    let dir = request_dir();
    std::fs::create_dir_all(&dir).map_err(|_| error())?;
    let id = format!(
        "{}-{}",
        std::process::id(),
        REQUEST_ID.fetch_add(1, Ordering::Relaxed)
    );
    let request = ExplorerRequest {
        files: paths,
        show_progress: true,
        to: Some(target),
        preset: None,
        source: "Cli",
        license: None,
        auth: None,
    };
    let bytes = serde_json::to_vec(&request).map_err(|_| error())?;
    let temp = dir.join(format!("request-{id}.tmp"));
    let path = dir.join(format!("request-{id}.json"));
    std::fs::write(&temp, bytes).map_err(|_| error())?;
    std::fs::rename(temp, path).map_err(|_| error())?;
    Ok(())
}
fn probe(path: &Path) -> Option<Vec<String>> {
    let mut child = Command::new(install_dir().ok()?.join("convt.exe"))
        .arg("targets")
        .arg(path)
        .arg("--menu")
        .creation_flags(CREATE_NO_WINDOW.0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(4097).read_to_end(&mut bytes).ok()?;
        (bytes.len() <= 4096).then_some(bytes)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < PROBE_LIMIT => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let bytes = reader.join().ok()??;
    if !status?.success() {
        return None;
    }
    let output = String::from_utf8(bytes).ok()?;
    let targets: Vec<_> = output.split_whitespace().map(str::to_owned).collect();
    targets
        .iter()
        .all(|id| {
            id.len() <= 32
                && id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
        .then_some(targets)
}

#[implement(IExplorerCommand, IObjectWithSite)]
struct ExplorerCommand {
    _lease: ModuleLease,
    target: Option<String>,
    site: Mutex<Option<IUnknown>>,
    selection: Mutex<Vec<PathBuf>>,
    children: Mutex<Vec<String>>,
}
impl ExplorerCommand {
    fn root() -> Self {
        Self {
            _lease: ModuleLease::new(),
            target: None,
            site: Mutex::new(None),
            selection: Mutex::new(vec![]),
            children: Mutex::new(vec![]),
        }
    }
}
impl IObjectWithSite_Impl for ExplorerCommand_Impl {
    fn SetSite(&self, site: Ref<IUnknown>) -> Result<()> {
        *self.site.lock().map_err(|_| error())? = site.cloned();
        Ok(())
    }
    fn GetSite(&self, iid: *const GUID, out: *mut *mut c_void) -> Result<()> {
        if iid.is_null() || out.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        unsafe {
            *out = std::ptr::null_mut();
        }
        let site = self.site.lock().map_err(|_| error())?;
        unsafe { site.as_ref().ok_or_else(error)?.query(iid, out).ok() }
    }
}
impl ExplorerCommand_Impl {
    fn selected_paths(&self) -> Result<Vec<PathBuf>> {
        let paths = self.selection.lock().map_err(|_| error())?.clone();
        if !paths.is_empty() {
            return Ok(paths);
        }
        // Some shell hosts enumerate before passing an item array to GetTitle
        // or GetState. Query the site's current view only during construction;
        // Invoke always uses the array supplied by the shell or this snapshot.
        let site = self
            .site
            .lock()
            .map_err(|_| error())?
            .clone()
            .ok_or_else(error)?;
        let services: IServiceProvider = site.cast()?;
        unsafe {
            let browser: IShellBrowser = services.QueryService(&SID_STopLevelBrowser)?;
            let view = browser.QueryActiveShellView()?;
            let items: IShellItemArray = view.GetItemObject(SVGIO_SELECTION)?;
            let paths = files((&items).into())?;
            *self.selection.lock().map_err(|_| error())? = paths.clone();
            Ok(paths)
        }
    }
}
impl IExplorerCommand_Impl for ExplorerCommand_Impl {
    fn GetTitle(&self, items: Ref<IShellItemArray>) -> Result<PWSTR> {
        if self.target.is_none()
            && let Ok(paths) = files(items)
        {
            *self.selection.lock().map_err(|_| error())? = paths;
        }
        text(
            &self
                .target
                .as_ref()
                .map_or_else(|| "Convert with convt".into(), |t| t.to_uppercase()),
        )
    }
    fn GetIcon(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        text(&format!(
            "{},0",
            install_dir()?.join("convt-app.exe").display()
        ))
    }
    fn GetToolTip(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        text("Convert locally with convt")
    }
    fn GetCanonicalName(&self) -> Result<GUID> {
        let mut id = CLSID;
        if let Some(target) = &self.target {
            // Stable distinct canonical names for each target, independent of menu order.
            id.data1 ^= target
                .bytes()
                .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
        }
        Ok(id)
    }
    fn GetState(&self, items: Ref<IShellItemArray>, slow: BOOL) -> Result<u32> {
        if self.target.is_some() {
            return Ok(ECS_ENABLED.0 as u32);
        }
        if !slow.as_bool() {
            return Err(Error::from_hresult(HRESULT(0x8000000Au32 as i32)));
        }
        let Ok(paths) = files(items) else {
            return Ok(ECS_HIDDEN.0 as u32);
        };
        let lists: Vec<_> = paths.iter().map(|p| targets(p)).collect();
        let targets = crate::common_targets(&lists);
        *self.selection.lock().map_err(|_| error())? = paths;
        let state = if targets.is_empty() {
            ECS_HIDDEN
        } else {
            ECS_ENABLED
        };
        *self.children.lock().map_err(|_| error())? = targets;
        Ok(state.0 as u32)
    }
    fn Invoke(&self, items: Ref<IShellItemArray>, _: Ref<IBindCtx>) -> Result<()> {
        let target = self.target.as_ref().ok_or_else(error)?;
        let paths = if items.is_some() {
            files(items)?
        } else {
            self.selection.lock().map_err(|_| error())?.clone()
        };
        if paths.is_empty() {
            return Err(error());
        }
        handoff(&paths, target)?;
        Command::new(install_dir()?.join("convt-app.exe"))
            .creation_flags(CREATE_NO_WINDOW.0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| error())?;
        Ok(())
    }
    fn GetFlags(&self) -> Result<u32> {
        Ok(if self.target.is_none() {
            ECF_HASSUBCOMMANDS.0 as u32
        } else {
            0
        })
    }
    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        let paths = self.selected_paths()?;
        let lists: Vec<_> = paths.iter().map(|p| targets(p)).collect();
        let targets = crate::common_targets(&lists);
        let commands = targets
            .iter()
            .map(|target| {
                ExplorerCommand {
                    _lease: ModuleLease::new(),
                    target: Some(target.clone()),
                    site: Mutex::new(None),
                    selection: Mutex::new(paths.clone()),
                    children: Mutex::new(vec![]),
                }
                .into()
            })
            .collect();
        Ok(Enumerator {
            _lease: ModuleLease::new(),
            commands,
            cursor: Mutex::new(0),
        }
        .into())
    }
}

#[implement(IEnumExplorerCommand)]
struct Enumerator {
    _lease: ModuleLease,
    commands: Vec<IExplorerCommand>,
    cursor: Mutex<usize>,
}
impl IEnumExplorerCommand_Impl for Enumerator_Impl {
    fn Next(
        &self,
        count: u32,
        commands: *mut Option<IExplorerCommand>,
        fetched: *mut u32,
    ) -> HRESULT {
        if commands.is_null() || (fetched.is_null() && count != 1) {
            return E_POINTER;
        }
        let Ok(mut cursor) = self.cursor.lock() else {
            return E_FAIL;
        };
        let take = (count as usize).min(self.commands.len() - *cursor);
        // COM caller provides count output slots. Each receives a new reference.
        unsafe {
            if !fetched.is_null() {
                *fetched = take as u32;
            }
            for i in 0..count as usize {
                commands.add(i).write(if i < take {
                    Some(self.commands[*cursor + i].clone())
                } else {
                    None
                });
            }
        }
        *cursor += take;
        if take == count as usize {
            S_OK
        } else {
            S_FALSE
        }
    }
    fn Skip(&self, count: u32) -> Result<()> {
        let mut cursor = self.cursor.lock().map_err(|_| error())?;
        let remaining = self.commands.len() - *cursor;
        *cursor += (count as usize).min(remaining);
        if count as usize > remaining {
            // The projection uses Result even though COM permits S_FALSE here.
            Err(Error::from_hresult(S_FALSE))
        } else {
            Ok(())
        }
    }
    fn Reset(&self) -> Result<()> {
        *self.cursor.lock().map_err(|_| error())? = 0;
        Ok(())
    }
    fn Clone(&self) -> Result<IEnumExplorerCommand> {
        Ok(Enumerator {
            _lease: ModuleLease::new(),
            commands: self.commands.clone(),
            cursor: Mutex::new(*self.cursor.lock().map_err(|_| error())?),
        }
        .into())
    }
}

#[implement(IClassFactory)]
struct Factory {
    _lease: ModuleLease,
}
impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<IUnknown>,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> Result<()> {
        if out.is_null() || iid.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        unsafe {
            *out = std::ptr::null_mut();
        }
        if outer.is_some() {
            return Err(Error::from_hresult(CLASS_E_NOAGGREGATION));
        }
        let command: IExplorerCommand = ExplorerCommand::root().into();
        unsafe { command.query(iid, out).ok() }
    }
    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            SERVER_LOCKS.fetch_add(1, Ordering::SeqCst);
        } else {
            let _ = SERVER_LOCKS.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                count.checked_sub(1)
            });
        }
        Ok(())
    }
}

/// COM export; Windows supplies valid GUIDs and an output pointer.
#[unsafe(no_mangle)]
unsafe extern "system" fn DllGetClassObject(
    class: *const GUID,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    if out.is_null() || class.is_null() || iid.is_null() {
        return E_POINTER;
    }
    unsafe {
        *out = std::ptr::null_mut();
        if *class != CLSID {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = Factory {
            _lease: ModuleLease::new(),
        }
        .into();
        factory.query(iid, out)
    }
}
// COM may release the DLL only after every object and server lock is gone.
// Probe workers join before the owning command returns.
#[unsafe(no_mangle)]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OBJECTS.load(Ordering::SeqCst) == 0 && SERVER_LOCKS.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}
