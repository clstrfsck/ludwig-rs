//! File I/O primitives for the Ludwig file command system.

use std::fmt;
use std::fs;
use std::io::{self, BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// State of one open file (input or output).
pub struct FileHandle {
    /// The target path (real file, not temp).
    pub path: PathBuf,
    /// Temporary output path (`path-lw`, `path-lw1`, …). None for input-only.
    pub temp_path: Option<PathBuf>,
    /// Line-buffered reader (input files only).
    pub reader: Option<BufReader<fs::File>>,
    /// Buffered writer (output files only).
    pub writer: Option<BufWriter<fs::File>>,
    /// Total lines read from this file so far.
    pub lines_read: usize,
    /// True when reader has reached EOF.
    pub at_eof: bool,
    /// Convert leading spaces → tabs (8-column) on write.
    pub entab: bool,
    /// How many backup versions to keep (0 = none).
    pub versions: usize,
    /// If true, purge oldest backups beyond `versions`.
    pub purge: bool,
}

impl fmt::Debug for FileHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileHandle")
            .field("path", &self.path)
            .field("temp_path", &self.temp_path)
            .field("lines_read", &self.lines_read)
            .field("at_eof", &self.at_eof)
            .field("entab", &self.entab)
            .field("versions", &self.versions)
            .field("purge", &self.purge)
            .finish_non_exhaustive()
    }
}

/// Open a file for reading.
pub fn open_input(path: &Path) -> io::Result<FileHandle> {
    let file = fs::File::open(path)?;
    Ok(FileHandle {
        path: path.to_path_buf(),
        temp_path: None,
        reader: Some(BufReader::new(file)),
        writer: None,
        lines_read: 0,
        at_eof: false,
        entab: false,
        versions: 1,
        purge: false,
    })
}

/// Open a file for writing, using a temp path to avoid overwriting until finalized.
pub fn open_output(
    path: &Path,
    entab: bool,
    versions: usize,
    purge: bool,
) -> io::Result<FileHandle> {
    let base = path.to_string_lossy().into_owned();
    let mut temp_path = PathBuf::from(format!("{}-lw", base));
    let mut i = 1usize;
    while temp_path.exists() {
        temp_path = PathBuf::from(format!("{}-lw{}", base, i));
        i += 1;
    }

    let file = fs::File::create(&temp_path)?;
    Ok(FileHandle {
        path: path.to_path_buf(),
        temp_path: Some(temp_path),
        reader: None,
        writer: Some(BufWriter::new(file)),
        lines_read: 0,
        at_eof: false,
        entab,
        versions,
        purge,
    })
}

/// Read all remaining content from a handle's reader, returning it as a String.
///
/// The returned string includes newlines. Sets `at_eof` when the reader is exhausted.
pub fn read_all(handle: &mut FileHandle) -> String {
    let mut result = String::new();
    if let Some(ref mut reader) = handle.reader {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    handle.at_eof = true;
                    break;
                }
                Ok(_) => {
                    handle.lines_read += 1;
                    result.push_str(&line);
                }
                Err(_) => {
                    handle.at_eof = true;
                    break;
                }
            }
        }
    } else {
        handle.at_eof = true;
    }
    result
}

/// Read up to `n` lines from a handle, returning them without trailing newlines.
///
/// Use `n = usize::MAX` to read until EOF.
/// Sets `at_eof` when the reader is exhausted.
pub fn read_lines(handle: &mut FileHandle, n: usize) -> Vec<String> {
    if handle.at_eof {
        return Vec::new();
    }
    let mut result = Vec::new();
    if let Some(ref mut reader) = handle.reader {
        let mut raw = String::new();
        let mut count = 0usize;
        loop {
            if count >= n {
                break;
            }
            raw.clear();
            match reader.read_line(&mut raw) {
                Ok(0) => {
                    handle.at_eof = true;
                    break;
                }
                Ok(_) => {
                    handle.lines_read += 1;
                    let content = raw
                        .trim_end_matches('\n')
                        .trim_end_matches('\r')
                        .to_string();
                    result.push(content);
                    count += 1;
                }
                Err(_) => {
                    handle.at_eof = true;
                    break;
                }
            }
        }
    } else {
        handle.at_eof = true;
    }
    result
}

/// Write all text to the output file.
pub fn write_all(handle: &mut FileHandle, text: &str) -> io::Result<()> {
    if let Some(ref mut writer) = handle.writer {
        writer.write_all(text.as_bytes())?;
    }
    Ok(())
}

/// Write a slice of lines to the output file, appending `\n` to each.
///
/// If `handle.entab` is set, converts leading 8-space runs to tabs.
pub fn write_lines(handle: &mut FileHandle, lines: &[String]) -> io::Result<()> {
    if let Some(ref mut writer) = handle.writer {
        for line in lines {
            let out = if handle.entab {
                entab_line(line)
            } else {
                line.clone()
            };
            writer.write_all(out.as_bytes())?;
            writer.write_all(b"\n")?;
        }
    }
    Ok(())
}

/// Convert leading runs of 8 spaces to tab characters.
///
/// Only leading whitespace is converted; the rest of the line is left unchanged.
pub fn entab_line(line: &str) -> String {
    let mut leading = 0usize;
    let rest = line.trim_start_matches(|c| {
        if c == ' ' {
            leading += 1;
            true
        } else {
            false
        }
    });
    let tabs = leading / 8;
    let rem = leading % 8;
    let mut result = String::with_capacity(tabs + rem + rest.len());
    for _ in 0..tabs {
        result.push('\t');
    }
    for _ in 0..rem {
        result.push(' ');
    }
    result.push_str(rest);
    result
}

/// Flush, create backups if requested, then rename temp → real path.
///
/// `create_backups`: if true and `handle.versions > 0` and the real path exists,
/// rotate existing backups up (e.g. `path~1` → `path~2`) and rename `path` → `path~1`.
pub fn finalize_output(handle: &mut FileHandle, create_backups: bool) -> io::Result<()> {
    // Flush and close the writer.
    if let Some(mut w) = handle.writer.take() {
        w.flush()?;
        // w is dropped here, closing the file.
    }

    let temp_path = match handle.temp_path.take() {
        Some(p) => p,
        None => return Ok(()),
    };

    // Create backup copies if requested.
    if create_backups && handle.versions > 0 && handle.path.exists() {
        // Shift existing backups up: path~(N-1) → path~N, ..., path~1 → path~2
        for i in (1..handle.versions).rev() {
            let from = backup_path(&handle.path, i);
            let to = backup_path(&handle.path, i + 1);
            if from.exists() {
                let _ = fs::rename(&from, &to);
            }
        }

        // Purge any extra backups beyond the version limit.
        if handle.purge {
            let mut n = handle.versions + 1;
            loop {
                let p = backup_path(&handle.path, n);
                if p.exists() {
                    let _ = fs::remove_file(&p);
                    n += 1;
                } else {
                    break;
                }
            }
        }

        // Rename the current real file to path~1.
        let backup1 = backup_path(&handle.path, 1);
        let _ = fs::rename(&handle.path, &backup1);
    }

    // Rename temp → real path.
    fs::rename(&temp_path, &handle.path)?;

    Ok(())
}

/// Delete the temp file without renaming it (used by FK / FGK / error paths).
pub fn delete_temp(handle: &FileHandle) {
    if let Some(ref tp) = handle.temp_path {
        let _ = fs::remove_file(tp);
    }
}

/// Rewind the reader to the beginning of the file.
///
/// Resets `at_eof` and `lines_read`.
pub fn rewind(handle: &mut FileHandle) -> io::Result<()> {
    handle.at_eof = false;
    handle.lines_read = 0;
    if let Some(ref mut reader) = handle.reader {
        reader.seek(SeekFrom::Start(0))?;
    }
    Ok(())
}

/// Build the path for backup version `n` (e.g. `path~1`, `path~2`).
fn backup_path(path: &Path, n: usize) -> PathBuf {
    PathBuf::from(format!("{}~{}", path.to_string_lossy(), n))
}
