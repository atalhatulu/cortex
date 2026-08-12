use std::fs::{self, File};
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// A writer that seamlessly splits the output into multiple files 
/// when a size threshold is reached. E.g. file, file.001, file.002
pub struct SplitWriter {
    base_path: PathBuf,
    current_file: File,
    split_size: u64,
    bytes_written_current: u64,
    part_index: u32,
}

impl SplitWriter {
    pub fn new<P: AsRef<Path>>(path: P, split_size: u64) -> io::Result<Self> {
        let current_file = File::create(&path)?;
        Ok(SplitWriter {
            base_path: path.as_ref().to_path_buf(),
            current_file,
            split_size: if split_size == 0 { u64::MAX } else { split_size },
            bytes_written_current: 0,
            part_index: 0,
        })
    }

    fn open_next_part(&mut self) -> io::Result<()> {
        self.part_index += 1;
        // e.g. path.ctx.001
        let new_path = format!("{}.{:03}", self.base_path.display(), self.part_index);
        self.current_file = File::create(new_path)?;
        self.bytes_written_current = 0;
        Ok(())
    }
}

impl Write for SplitWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.bytes_written_current >= self.split_size {
            self.open_next_part()?;
        }

        let remaining = self.split_size.saturating_sub(self.bytes_written_current);
        let to_write = std::cmp::min(buf.len() as u64, remaining) as usize;
        let to_write = if to_write == 0 { buf.len() } else { to_write };

        let n = self.current_file.write(&buf[..to_write])?;
        self.bytes_written_current += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.current_file.flush()
    }
}

/// A reader that reads across multiple files transparently.
pub struct SplitReader {
    base_path: PathBuf,
    current_file: File,
    part_index: u32,
}

/// True when `name` looks like a split-volume suffix, e.g. `archive.ctx.001`.
fn is_split_part(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 4
        && b[b.len() - 4] == b'.'
        && b[b.len() - 3..].iter().all(|c| c.is_ascii_digit())
}

impl SplitReader {
    /// Open an archive for reading, even when pointed at any one of its split
    /// volumes (`archive.ctx.001`, `archive.ctx.002`, …).
    ///
    /// The header (magic bytes + metadata) lives only in the base file
    /// (`archive.ctx`), so a part-path is normalized to that base path. Reading
    /// therefore always starts from the beginning and chains the parts in
    /// order, no matter which volume the user handed us.
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let given = path.as_ref();
        let base_path = match given.file_name().and_then(|s| s.to_str()) {
            Some(name) if is_split_part(name) => {
                // Drop the trailing ".NNN" (4 chars): archive.ctx.001 -> archive.ctx
                let trimmed = &name[..name.len() - 4];
                given.with_file_name(trimmed)
            }
            _ => given.to_path_buf(),
        };
        let current_file = File::open(&base_path)?;
        Ok(SplitReader {
            base_path,
            current_file,
            part_index: 0,
        })
    }

    fn try_open_next_part(&mut self) -> io::Result<bool> {
        let next_index = self.part_index + 1;
        let next_path = format!("{}.{:03}", self.base_path.display(), next_index);
        if Path::new(&next_path).exists() {
            self.current_file = File::open(&next_path)?;
            self.part_index = next_index;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Total size in bytes across all volumes (base + `.001`, `.002`, …).
    /// Used only for reporting (e.g. the CLI "Input:" stat) — it never
    /// affects decode correctness.
    pub fn total_size(&self) -> io::Result<u64> {
        let mut total = self.current_file.metadata()?.len();
        let mut idx = 1u32;
        loop {
            let part = format!("{}.{:03}", self.base_path.display(), idx);
            match fs::metadata(&part) {
                Ok(m) => {
                    total += m.len();
                    idx += 1;
                }
                Err(_) => break,
            }
        }
        Ok(total)
    }
}

impl Read for SplitReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut total_read = 0;
        while total_read < buf.len() {
            let n = self.current_file.read(&mut buf[total_read..])?;
            if n == 0 {
                // EOF of current file, try next
                if !self.try_open_next_part()? {
                    break;
                }
            } else {
                total_read += n;
            }
        }
        Ok(total_read)
    }
}

impl Seek for SplitReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.current_file.seek(pos)
    }
}
