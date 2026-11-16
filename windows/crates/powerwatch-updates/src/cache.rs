//! Bounded installer cache. Only files created by our downloader are owned here.
use crate::ReleaseAsset;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
fn version(path: &Path) -> Option<semver::Version> {
    if !matches!(path.extension()?.to_str()?, "apk" | "exe") {
        return None;
    }
    let (release, sequence) = path.file_stem()?.to_str()?.rsplit_once('-')?;
    if sequence.is_empty() || !sequence.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    semver::Version::parse(release).ok()
}
/// Installed/older packages are unnecessary. Keep at most one future installer
/// for seven days, or the explicitly retained package being handed to the OS.
/// Never follow links, recurse, or touch unrelated files in the cache directory.
pub fn prune_installers(
    directory: &Path,
    current: &str,
    keep: Option<&Path>,
    now: SystemTime,
) -> io::Result<u64> {
    let current = semver::Version::parse(current).map_err(io::Error::other)?;
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(release) = version(&path) else {
            continue;
        };
        let metadata = entry.metadata()?;
        files.push((path, release, metadata.modified()?, metadata.len()));
    }
    files.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.0.cmp(&a.0)));
    let mut retained = keep.is_some();
    let mut freed = 0;
    for (path, release, modified, length) in files {
        if keep == Some(path.as_path()) {
            continue;
        }
        let expired =
            now.duration_since(modified).unwrap_or_default() >= Duration::from_secs(7 * 86400);
        if keep.is_none() && !retained && release > current && !expired {
            retained = true;
            continue;
        }
        if fs::remove_file(path).is_ok() {
            freed += length;
        }
    }
    Ok(freed)
}
/// Reuse after process death only if the verified manifest still matches every
/// byte. An interrupted download cannot become an installable cached package.
pub fn cached_installer(
    directory: &Path,
    release: &str,
    asset: &ReleaseAsset,
) -> io::Result<Option<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() || entry.metadata()?.len() != asset.size {
            continue;
        }
        let path = entry.path();
        if version(&path).is_none_or(|value| value.to_string() != release) {
            continue;
        }
        let mut input = fs::File::open(&path)?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 65536];
        loop {
            let length = input.read(&mut buffer)?;
            if length == 0 {
                break;
            }
            digest.update(&buffer[..length]);
        }
        if format!("{:x}", digest.finalize()) == asset.sha256.to_ascii_lowercase() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}
