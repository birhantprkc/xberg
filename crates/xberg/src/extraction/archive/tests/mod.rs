use super::*;
use crate::extractors::security::SecurityLimits;

fn default_limits() -> SecurityLimits {
    SecurityLimits::default()
}

fn entry(path: &str) -> ArchiveEntry {
    ArchiveEntry {
        path: path.to_string(),
        size: 0,
        is_dir: false,
    }
}

mod formats;
mod gzip_and_limits;
