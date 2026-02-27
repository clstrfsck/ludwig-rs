//! Shared file-save helper used by both batch mode and interactive Ctrl-S.

use std::io;
use std::path::Path;

/// Write `contents` to `path`, rotating up to `versions` backups.
///
/// Backups are named `path~1`, `path~2`, …  Existing backups are rotated
/// before writing: `path~(versions-1)` → `path~versions`, …, `path~1` →
/// `path~2`, `path` → `path~1`.
///
/// Returns the number of lines written on success.
pub fn write_with_backup(contents: &str, path: &str, versions: usize) -> io::Result<usize> {
    let dest = Path::new(path);

    // Rotate existing backups highest → lowest to avoid clobbering.
    for v in (1..versions).rev() {
        let old = format!("{}~{}", path, v);
        let new = format!("{}~{}", path, v + 1);
        if Path::new(&old).exists() {
            let _ = std::fs::rename(&old, &new);
        }
    }

    // Rename current file to ~1 backup.
    if versions >= 1 && dest.exists() {
        let backup = format!("{}~1", path);
        std::fs::rename(dest, &backup)?;
    }

    // Ensure contents end with a newline.
    let body = if contents.is_empty() || contents.ends_with('\n') {
        contents.to_string()
    } else {
        format!("{}\n", contents)
    };

    std::fs::write(dest, &body)?;

    Ok(body.lines().count())
}
