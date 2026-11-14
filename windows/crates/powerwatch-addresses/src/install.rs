use crate::{Catalog, CatalogError, Result};
use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogManifest {
    pub schema: u32,
    pub sha256: String,
    pub bytes: u64,
}
/// Install a verified immutable version once. Subsequent opens read SQLite
/// directly; user data and older catalog versions are never rewritten.
pub fn install(
    directory: &Path,
    compressed: impl Read,
    manifest: &CatalogManifest,
) -> Result<PathBuf> {
    if manifest.schema != 1
        || manifest.sha256.len() != 64
        || !manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || manifest.bytes > 4 * 1024 * 1024 * 1024
    {
        return Err(CatalogError::InvalidPackage);
    }
    fs::create_dir_all(directory)?;
    let target = directory.join(format!("addresses-{}.sqlite3", manifest.sha256));
    if target.exists() {
        if fs::metadata(&target)?.len() != manifest.bytes {
            return Err(CatalogError::InvalidPackage);
        }
        Catalog::open(&target, &manifest.sha256)?;
        return Ok(target);
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = directory.join(format!(
        "addresses-{}-{}-{}.part",
        manifest.sha256,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        let mut input = GzDecoder::new(compressed).take(manifest.bytes + 1);
        let mut hash = Sha256::new();
        let mut length = 0_u64;
        let mut buffer = [0_u8; 65536];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            length += n as u64;
            hash.update(&buffer[..n]);
            output.write_all(&buffer[..n])?;
        }
        if length != manifest.bytes || format!("{:x}", hash.finalize()) != manifest.sha256 {
            return Err(CatalogError::InvalidPackage);
        }
        output.sync_all()?;
        drop(output);
        Catalog::open(&temp, &manifest.sha256)?;
        if let Err(error) = fs::rename(&temp, &target) {
            if !target.exists() || File::open(&target)?.metadata()?.len() != manifest.bytes {
                return Err(error.into());
            }
            Catalog::open(&target, &manifest.sha256)?;
        }
        Ok(target)
    })();
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}
