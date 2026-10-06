//! Fail-closed checks for executable document packs on local Windows filesystems.
use anyhow::bail;
use std::{
    ffi::c_void,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree},
    Security::{
        Authorization::{ConvertStringSidToSidW, GetSecurityInfo, SE_FILE_OBJECT},
        *,
    },
    Storage::FileSystem::{
        DELETE, FILE_APPEND_DATA, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA,
        READ_CONTROL, WRITE_DAC, WRITE_OWNER,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

struct Descriptor(PSECURITY_DESCRIPTOR);
impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: Windows allocated this descriptor with LocalAlloc.
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn current_sid() -> anyhow::Result<Vec<u32>> {
    // SAFETY: valid output pointers; token buffer remains live until its SID is copied.
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut size = 0;
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut size);
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let ok = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        );
        CloseHandle(token);
        if ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let length = GetLengthSid(user.User.Sid);
        let mut sid = vec![0u32; (length as usize).div_ceil(4)];
        if CopySid(length, sid.as_mut_ptr().cast(), user.User.Sid) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(sid)
    }
}

fn system_sid(kind: WELL_KNOWN_SID_TYPE) -> anyhow::Result<Vec<u32>> {
    let mut sid = vec![0u32; 17];
    let mut size = 68;
    // SAFETY: buffer has SECURITY_MAX_SID_SIZE bytes and valid length pointer.
    if unsafe { CreateWellKnownSid(kind, ptr::null_mut(), sid.as_mut_ptr().cast(), &mut size) } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(sid)
}

/// Ancestors may be system-owned and allow users to create siblings, but may
/// not let another user replace an existing child. Pack objects permit our user and privileged system owners.
pub(crate) fn trusted(path: &Path, ancestor: bool) -> anyhow::Result<()> {
    // Open the object itself, without following its reparse tag, and inspect
    // the same handle's attributes and security descriptor.
    let file = std::fs::OpenOptions::new()
        .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let metadata = file.metadata()?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        bail!("document-pack reparse point forbidden: {}", path.display());
    }
    let mut owner = ptr::null_mut();
    let mut acl = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    // SAFETY: live object handle and valid out-pointers; descriptor owns returned SID/ACL.
    let error = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if error != 0 {
        return Err(std::io::Error::from_raw_os_error(error as i32).into());
    }
    let _descriptor = Descriptor(descriptor);
    if acl.is_null() || owner.is_null() {
        bail!(
            "document-pack path has no restrictive ACL: {}",
            path.display()
        );
    }
    let user = current_sid()?;
    let system = system_sid(WinLocalSystemSid)?;
    let admins = system_sid(WinBuiltinAdministratorsSid)?;
    // The drive root is normally owned by Windows Modules Installer.
    let installer_name: Vec<u16> = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut installer = ptr::null_mut();
    // SAFETY: valid NUL-terminated SID string and output pointer; LocalFree below.
    if unsafe { ConvertStringSidToSidW(installer_name.as_ptr(), &mut installer) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let _installer = Descriptor(installer);

    let same = |a: PSID, b: &[u32]| {
        // SAFETY: all SID pointers are supplied by Windows security APIs.
        unsafe { EqualSid(a, b.as_ptr().cast_mut().cast()) != 0 }
    };
    let privileged = |sid| same(sid, &user) || same(sid, &system) || same(sid, &admins);
    if !privileged(owner) && !(ancestor && unsafe { EqualSid(owner, installer) != 0 }) {
        bail!(
            "document-pack path has an untrusted owner: {}",
            path.display()
        );
    }
    let mut writes = DELETE
        | WRITE_DAC
        | WRITE_OWNER
        | FILE_DELETE_CHILD
        | FILE_WRITE_EA
        | FILE_WRITE_ATTRIBUTES
        | 0x10000000
        | 0x40000000;
    if !ancestor {
        writes |= FILE_WRITE_DATA | FILE_APPEND_DATA | FILE_WRITE_EA | FILE_WRITE_ATTRIBUTES;
    }
    // SAFETY: GetAce returns each ACE inside the live security descriptor. Basic
    // allow/deny ACE layouts are checked before reading their SID and access mask.
    unsafe {
        for index in 0..u32::from((*acl).AceCount) {
            let mut ace: *mut c_void = ptr::null_mut();
            if GetAce(acl, index, &mut ace) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let header = &*ace.cast::<ACE_HEADER>();
            if ancestor && u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0 {
                continue;
            }
            match header.AceType {
                0 => {
                    let allow = &*ace.cast::<ACCESS_ALLOWED_ACE>();
                    let sid = ptr::addr_of!(allow.SidStart).cast_mut().cast();
                    if allow.Mask & writes != 0 && !privileged(sid) {
                        bail!(
                            "document-pack path is writable by another principal: {}",
                            path.display()
                        );
                    }
                }
                1 => {} // Denies cannot grant write permission.
                _ => bail!(
                    "document-pack ACL contains an unsupported ACE: {}",
                    path.display()
                ),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn private_pack_rejects_other_users_write_access() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("launcher.exe");
        std::fs::write(&file, b"not executable").unwrap();
        trusted(temp.path(), false).unwrap();
        trusted(&file, false).unwrap();
        let result = Command::new("icacls")
            .arg(&file)
            .args(["/grant", "*S-1-1-0:(M)"])
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        assert!(trusted(&file, false).is_err());
    }

    #[test]
    fn ancestor_can_allow_sibling_creation_but_not_child_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let result = Command::new("icacls")
            .arg(temp.path())
            .args(["/grant", "*S-1-1-0:(AD)"])
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        trusted(temp.path(), true).unwrap();
        assert!(trusted(temp.path(), false).is_err());
        let result = Command::new("icacls")
            .arg(temp.path())
            .args(["/grant", "*S-1-1-0:(DC)"])
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        assert!(trusted(temp.path(), true).is_err());
    }

    #[test]
    fn junctions_are_rejected_without_following_them() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        let junction = temp.path().join("junction");
        std::fs::create_dir(&real).unwrap();
        let result = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&real)
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        assert!(trusted(&junction, false).is_err());
        std::fs::remove_dir(junction).unwrap();
    }
}
