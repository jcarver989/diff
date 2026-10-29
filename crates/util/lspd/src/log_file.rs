use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{self, Write};
use std::path::Path;

pub(crate) struct LogFile {
    file: File,
    limit: u64,
}

impl LogFile {
    pub(crate) fn open(path: &Path, limit: u64) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { file, limit })
    }
}

impl Write for &LogFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.file.metadata()?.len() > self.limit {
            self.file.set_len(0)?;
        }
        (&self.file).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.file).flush()
    }
}
