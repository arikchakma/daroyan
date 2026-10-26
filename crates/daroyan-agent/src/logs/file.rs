use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use nix::libc::O_NOFOLLOW;

use daroyan_protocol::log::LogStream;
use serde::{Deserialize, Serialize};

use crate::logs::parse::RawLine;

const MAX_READ: u64 = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilePosition {
    pub dev: u64,
    pub ino: u64,
    pub offset: u64,
}

pub fn read_file(
    path: &Path,
    position: &mut Option<FilePosition>,
    now_ms: u64,
) -> Vec<RawLine> {
    let Some((mut file, metadata)) = open_regular(path) else {
        return Vec::new();
    };
    let (dev, ino, size) = (metadata.dev(), metadata.ino(), metadata.len());
    let Some(current) = *position else {
        *position = Some(FilePosition { dev, ino, offset: size });
        return Vec::new();
    };
    let start = if current.dev != dev || current.ino != ino || size < current.offset {
        0
    } else {
        current.offset
    };
    if size == start {
        *position = Some(FilePosition { dev, ino, offset: start });
        return Vec::new();
    }
    let mut buffer = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_err()
        || file
            .take((size - start).min(MAX_READ))
            .read_to_end(&mut buffer)
            .is_err()
    {
        return Vec::new();
    }
    let Some(end) = buffer.iter().rposition(|byte| *byte == b'\n') else {
        return Vec::new();
    };
    *position = Some(FilePosition { dev, ino, offset: start + end as u64 + 1 });
    String::from_utf8_lossy(&buffer[..end])
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| RawLine {
            at: now_ms,
            stream: LogStream::File,
            text: line.to_string(),
            severity: None,
        })
        .collect()
}

fn open_regular(path: &Path) -> Option<(File, std::fs::Metadata)> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    (metadata.is_file() && metadata.nlink() == 1).then_some((file, metadata))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use super::read_file;

    #[test]
    fn follows_appends_partial_lines_and_rotation() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-file-log-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app.log");
        fs::write(&path, "old line\n").unwrap();
        let mut position = None;
        assert!(read_file(&path, &mut position, 1).is_empty());

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(file, "first\nsecond\npart").unwrap();
        let lines = read_file(&path, &mut position, 2);
        assert_eq!(
            lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );
        writeln!(file, "ial").unwrap();
        assert_eq!(read_file(&path, &mut position, 3)[0].text, "partial");

        fs::rename(&path, dir.join("app.log.1")).unwrap();
        fs::write(&path, "after rotation\n").unwrap();
        assert_eq!(read_file(&path, &mut position, 4)[0].text, "after rotation");

        let secret = dir.join("secret");
        fs::write(&secret, "root only\n").unwrap();
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&secret, &path).unwrap();
        assert!(read_file(&path, &mut position, 5).is_empty());
        fs::remove_file(&path).unwrap();
        fs::hard_link(&secret, &path).unwrap();
        assert!(read_file(&path, &mut position, 6).is_empty());
        fs::remove_dir_all(dir).ok();
    }
}
