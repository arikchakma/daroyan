use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use flexi_logger::{
    Age, Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming,
};

pub fn start_logger(level: &str, path: &Path) -> Result<LoggerHandle> {
    fs::create_dir_all(path)
        .with_context(|| format!("create the log directory {}", path.display()))?;

    let handle = Logger::try_with_str(level)?
        .log_to_file(FileSpec::default().directory(path))
        .rotate(
            Criterion::AgeOrSize(Age::Day, 20 * 1024 * 1024),
            Naming::Timestamps,
            Cleanup::KeepLogFiles(7),
        )
        .append()
        .create_symlink(path.join("daroyan.log"))
        .format_for_files(flexi_logger::detailed_format)
        .duplicate_to_stdout(Duplicate::All)
        .format_for_stdout(flexi_logger::detailed_format)
        .cleanup_in_background_thread(true)
        .start()?;

    Ok(handle)
}
