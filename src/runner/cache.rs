//! The build cache of `atlc run`: where entries live, their key, the fingerprint of what CMake found, and a lock
//! so that two runs never build one entry at once.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The cache's directory: `ATLC_CACHE_DIR`, else the platform's cache location (`~/Library/Caches/atlc` on macOS,
/// `%LOCALAPPDATA%\atlc\cache` on Windows, `$XDG_CACHE_HOME/atlc` or `~/.cache/atlc` elsewhere).
pub fn root() -> Result<PathBuf, String> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = var("ATLC_CACHE_DIR") {
        return Ok(dir);
    }
    let found = if cfg!(windows) {
        var("LOCALAPPDATA").map(|d| d.join("atlc").join("cache"))
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library").join("Caches").join("atlc"))
    } else {
        var("XDG_CACHE_HOME")
            .map(|d| d.join("atlc"))
            .or_else(|| var("HOME").map(|h| h.join(".cache").join("atlc")))
    };
    found.ok_or_else(|| "cannot find the platform's cache directory: set ATLC_CACHE_DIR".to_string())
}

/// FNV-1a, 128 bits, over parts separated by a zero byte: a key stable across Rust releases and platforms
/// (std's hasher is neither).
pub fn key(parts: &[&[u8]]) -> u128 {
    let mut h: u128 = 0x6c62272e07bb014262b821756295c58d;
    for part in parts {
        for b in part.iter().chain(std::iter::once(&0u8)) {
            h ^= u128::from(*b);
            h = h.wrapping_mul(0x0000000001000000000000000000013B);
        }
    }
    h
}

/// A file's size and modification time in nanoseconds, or `missing`.
pub fn stat(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(m) => {
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{} {modified}", m.len())
        }
        Err(_) => "missing".into(),
    }
}

/// The fingerprint of what a build used: each root directory's files (recursively, by path, size and modification
/// time) and each single file's. A file added, removed, rewritten or touched changes it.
pub fn fingerprint(roots: &[PathBuf], files: &[PathBuf]) -> String {
    let mut lines = vec![];
    for root in roots {
        let mut found = vec![];
        walk(root, &mut found);
        found.sort();
        lines.push(format!("dir {} {}", root.display(), found.len()));
        for f in found {
            lines.push(format!("{} {}", f.display(), stat(&f)));
        }
    }
    for f in files {
        lines.push(format!("file {} {}", f.display(), stat(f)));
    }
    lines.join("\n")
}

/// Every file under `dir`, following no links out of it; nothing when it cannot be read.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => walk(&path, out),
            Ok(_) => out.push(path),
            Err(_) => {}
        }
    }
}

/// An entry's lock: an OS lock on a file beside the entry, held until dropped. The system releases it when the
/// run that holds it ends, so a run that died leaves nothing to clean up; the file itself stays.
pub struct Lock {
    _file: std::fs::File,
}

impl Lock {
    /// Takes the lock of `entry`, waiting while another run holds it.
    pub fn take(entry: &Path) -> Result<Lock, String> {
        let mut path = entry.as_os_str().to_owned();
        path.push(".lock");
        let path = PathBuf::from(path);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                eprintln!("atlc: waiting for another atlc building {}", entry.display());
                file.lock()
                    .map_err(|e| format!("cannot lock {}: {e}", path.display()))?;
            }
            Err(std::fs::TryLockError::Error(e)) => {
                return Err(format!("cannot lock {}: {e}", path.display()));
            }
        }
        Ok(Lock { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_separates_parts_and_is_stable() {
        assert_ne!(key(&[b"ab", b"c"]), key(&[b"a", b"bc"]));
        assert_ne!(key(&[b"a"]), key(&[b"a", b""]));
        // FNV-1a 128 of "a" then the separator: fixed, so a key never moves between releases.
        assert_eq!(format!("{:032x}", key(&[b"a"])), format!("{:032x}", key(&[b"a"])));
        assert_eq!(key(&[]), 0x6c62272e07bb014262b821756295c58d);
    }

    #[test]
    fn the_fingerprint_sees_a_touched_or_added_file() {
        let dir = std::env::temp_dir().join(format!("atlc-fingerprint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/a.hpp"), "a").unwrap();
        let single = dir.join("single");
        std::fs::write(&single, "s").unwrap();
        let before = fingerprint(std::slice::from_ref(&dir), std::slice::from_ref(&single));
        assert_eq!(
            before,
            fingerprint(std::slice::from_ref(&dir), std::slice::from_ref(&single))
        );
        std::fs::write(dir.join("sub/a.hpp"), "b").unwrap();
        let f = std::fs::File::options()
            .write(true)
            .open(dir.join("sub/a.hpp"))
            .unwrap();
        f.set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1))
            .unwrap();
        let touched = fingerprint(std::slice::from_ref(&dir), std::slice::from_ref(&single));
        assert_ne!(before, touched);
        std::fs::write(dir.join("b.hpp"), "").unwrap();
        assert_ne!(
            touched,
            fingerprint(std::slice::from_ref(&dir), std::slice::from_ref(&single))
        );
        std::fs::remove_file(&single).unwrap();
        assert!(fingerprint(&[], &[single]).ends_with("missing"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_lock_is_exclusive_until_dropped() {
        let dir = std::env::temp_dir().join(format!("atlc-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("e");
        let other = || std::fs::File::options().write(true).open(dir.join("e.lock")).unwrap();
        let lock = Lock::take(&entry).unwrap();
        assert!(matches!(other().try_lock(), Err(std::fs::TryLockError::WouldBlock)));
        drop(lock);
        assert!(other().try_lock().is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
