//! File paths that come from documents.
//!
//! A path stored in a file (a linked bitmap, an SVG image reference) is
//! chosen by whoever made that file. Reading it can reach the network: on
//! Windows a UNC path (`\\host\share\x.png`) makes the system connect to
//! that host and sign in with the user's credentials. Such paths are only
//! followed when the user asks for it.

/// True when `path` names a network location or a device rather than a
/// file on a local drive: `\\host\share`, `//host/share`,
/// `\\?\UNC\host\share`, `\\.\device`. A long local path (`\\?\C:\...`)
/// stays local.
pub fn is_remote_path(path: &str) -> bool {
    let p = path.trim();
    let b = p.as_bytes();
    let sep = |c: u8| c == b'\\' || c == b'/';
    if b.len() < 2 || !sep(b[0]) || !sep(b[1]) {
        return false;
    }
    // `\\?\` + drive letter + `:` is a local path in long form.
    let long_local =
        b.len() >= 6 && b[2] == b'?' && sep(b[3]) && b[4].is_ascii_alphabetic() && b[5] == b':';
    !long_local
}

#[cfg(test)]
mod tests {
    use super::is_remote_path;

    #[test]
    fn network_and_device_paths_are_remote() {
        for p in [
            r"\\host\share\a.png",
            "//host/share/a.png",
            r"\/host\a.png",
            r"\\?\UNC\host\share\a.png",
            r"\\.\pipe\x",
            r"  \\host\a.png",
            r"\\",
        ] {
            assert!(is_remote_path(p), "{p}");
        }
    }

    #[test]
    fn local_paths_are_not_remote() {
        for p in [
            r"C:\Users\a\b.png",
            "/home/a/b.png",
            "b.png",
            "./b.png",
            r"\\?\C:\long\b.png",
            "",
            r"\a.png",
        ] {
            assert!(!is_remote_path(p), "{p}");
        }
    }
}
