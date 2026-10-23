//! Secrets kept in the settings file (the AI service's remembered key).
//!
//! On Windows a secret is written encrypted for the signed-in user
//! (DPAPI, `dpapi:` and base64): a copy of the file on another account or
//! machine, a backup or a synced folder does not give the key away.
//! Elsewhere the settings file itself is readable by its owner only
//! (`write_private`). A key written by an older version, in the clear, is
//! still read and is encrypted on the next save.

/// What marks an encrypted secret in the file.
const PREFIX: &str = "dpapi:";

/// `plain` as it goes into the settings file.
pub fn seal(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    #[cfg(windows)]
    {
        use base64::Engine;
        if let Some(bytes) = dpapi::protect(plain.as_bytes()) {
            return format!(
                "{PREFIX}{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            );
        }
        log::warn!("could not encrypt the remembered key; it is not saved");
        String::new()
    }
    #[cfg(not(windows))]
    plain.to_string()
}

/// The secret `stored` in the settings file holds: decrypted when it was
/// encrypted (empty when that fails, as on another account), else as is.
pub fn open(stored: &str) -> String {
    let Some(sealed) = stored.strip_prefix(PREFIX) else {
        return stored.to_string();
    };
    #[cfg(windows)]
    {
        use base64::Engine;
        let plain = base64::engine::general_purpose::STANDARD
            .decode(sealed.trim())
            .ok()
            .and_then(|b| dpapi::unprotect(&b))
            .and_then(|b| String::from_utf8(b).ok());
        if plain.is_none() {
            log::warn!("the remembered key could not be decrypted here; enter it again");
        }
        plain.unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        let _ = sealed;
        log::warn!("the remembered key was encrypted on Windows; enter it again");
        String::new()
    }
}

/// Write `data` to `path` so that only its owner can read it (mode 0600
/// on Unix, also when the file was there before). Windows files under the
/// user's profile are the user's already.
pub fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        if path.exists() {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(data)
    }
    #[cfg(not(unix))]
    std::fs::write(path, data)
}

#[cfg(windows)]
mod dpapi {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    #[repr(C)]
    struct Blob {
        len: u32,
        data: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            data_in: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            data_out: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            data_in: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            data_out: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(mem: *mut c_void) -> *mut c_void;
    }

    /// Never show a dialog.
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;
    /// Mixed into the encryption so the blob opens for TraceDraw's key only.
    const ENTROPY: &[u8] = b"TraceDraw remembered key";

    fn run(input: &[u8], seal: bool) -> Option<Vec<u8>> {
        let mut input = input.to_vec();
        let mut entropy = ENTROPY.to_vec();
        let blob_in = Blob {
            len: u32::try_from(input.len()).ok()?,
            data: input.as_mut_ptr(),
        };
        let blob_entropy = Blob {
            len: entropy.len() as u32,
            data: entropy.as_mut_ptr(),
        };
        let mut out = Blob {
            len: 0,
            data: null_mut(),
        };
        // SAFETY: the blobs point at buffers that live past the call; the
        // output is copied and released with LocalFree, as the API asks.
        unsafe {
            let ok = if seal {
                CryptProtectData(
                    &blob_in,
                    null(),
                    &blob_entropy,
                    null_mut(),
                    null_mut(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            } else {
                CryptUnprotectData(
                    &blob_in,
                    null_mut(),
                    &blob_entropy,
                    null_mut(),
                    null_mut(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            };
            if ok == 0 || out.data.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(out.data, out.len as usize).to_vec();
            LocalFree(out.data.cast());
            Some(bytes)
        }
    }

    pub fn protect(plain: &[u8]) -> Option<Vec<u8>> {
        run(plain, true)
    }

    pub fn unprotect(sealed: &[u8]) -> Option<Vec<u8>> {
        run(sealed, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key goes into the file and comes back the same; on Windows it is
    /// not in the file in the clear.
    #[test]
    fn a_sealed_key_opens_again() {
        let key = "sk-test-0123456789";
        let stored = seal(key);
        assert_eq!(open(&stored), key);
        if cfg!(windows) {
            assert!(stored.starts_with(PREFIX));
            assert!(!stored.contains(key));
        }
        assert_eq!(seal(""), "");
        assert_eq!(open(""), "");
    }

    /// Keys saved in the clear by older versions are still read; a
    /// damaged encrypted one gives no key instead of garbage.
    #[test]
    fn older_and_damaged_keys() {
        assert_eq!(open("sk-plain"), "sk-plain");
        assert_eq!(open("dpapi:not base64!"), "");
        assert_eq!(open("dpapi:AAAA"), "");
    }

    #[cfg(unix)]
    #[test]
    fn private_files_are_the_owners_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("tracedraw-secret-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("settings.json");
        std::fs::write(&p, b"{}").expect("write");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        write_private(&p, b"{\"a\":1}").expect("private");
        let mode = std::fs::metadata(&p).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(std::fs::read(&p).expect("read"), b"{\"a\":1}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
