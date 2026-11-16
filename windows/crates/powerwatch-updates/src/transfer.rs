//! Resumable transfer of one immutable asset from a verified release manifest.
use crate::{MAX_PACKAGE, ReleaseAsset, ReleaseClient, UpdateError, valid_asset_url};
use reqwest::{StatusCode, header};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    time::Duration,
};
#[derive(Clone, Copy)]
pub enum TransferStage {
    Downloading,
    Waiting,
    Verifying,
}
impl ReleaseClient {
    pub fn download(
        &self,
        asset: &ReleaseAsset,
        repository: &str,
        path: &Path,
        progress: impl FnMut(u64, u64),
    ) -> Result<(), UpdateError> {
        self.download_with_status(asset, repository, path, progress, |_| {})
    }
    pub fn download_with_status(
        &self,
        asset: &ReleaseAsset,
        repository: &str,
        path: &Path,
        progress: impl FnMut(u64, u64),
        stage: impl FnMut(TransferStage),
    ) -> Result<(), UpdateError> {
        if !valid_asset_url(&asset.url, repository) || asset.size == 0 || asset.size > MAX_PACKAGE {
            return Err(UpdateError::Invalid);
        }
        self.download_from(asset, path, progress, stage, Duration::from_secs(2))
    }
    fn download_from(
        &self,
        asset: &ReleaseAsset,
        path: &Path,
        mut progress: impl FnMut(u64, u64),
        mut stage: impl FnMut(TransferStage),
        delay: Duration,
    ) -> Result<(), UpdateError> {
        if path.try_exists().map_err(|_| UpdateError::Storage)? {
            return Err(UpdateError::Storage);
        }
        let partial = path.with_extension(format!(
            "{}.part",
            path.extension()
                .and_then(|s| s.to_str())
                .ok_or(UpdateError::Invalid)?
        ));
        if fs::symlink_metadata(&partial).is_ok_and(|m| !m.file_type().is_file()) {
            return Err(UpdateError::Storage);
        }
        let mut result = Err(UpdateError::Network);
        for attempt in 0..3 {
            stage(TransferStage::Downloading);
            result = self.transfer_once(asset, &partial, &mut progress, &mut stage);
            if !matches!(result, Err(UpdateError::Network)) || attempt == 2 {
                break;
            }
            stage(TransferStage::Waiting);
            std::thread::sleep(delay * (attempt + 1));
        }
        match result {
            Ok(()) => fs::rename(&partial, path).map_err(|_| UpdateError::Storage),
            Err(UpdateError::Integrity) => {
                let _ = fs::remove_file(&partial);
                Err(UpdateError::Integrity)
            }
            Err(error) => Err(error),
        }
    }
    fn transfer_once(
        &self,
        asset: &ReleaseAsset,
        partial: &Path,
        progress: &mut impl FnMut(u64, u64),
        stage: &mut impl FnMut(TransferStage),
    ) -> Result<(), UpdateError> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(partial)
            .map_err(|_| UpdateError::Storage)?;
        let mut size = file.metadata().map_err(|_| UpdateError::Storage)?.len();
        if size > asset.size {
            return Err(UpdateError::Integrity);
        }
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 65536];
        loop {
            let count = file.read(&mut buffer).map_err(|_| UpdateError::Storage)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        progress(size, asset.size);
        if size < asset.size {
            let mut request = self.0.get(&asset.url).timeout(Duration::from_secs(300));
            if size > 0 {
                request = request.header(header::RANGE, format!("bytes={size}-"));
            }
            let response = request.send().map_err(|_| UpdateError::Network)?;
            let mut response = Self::checked_response(response, true)?;
            if response.status() == StatusCode::PARTIAL_CONTENT {
                if !valid_range(
                    response
                        .headers()
                        .get(header::CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok()),
                    size,
                    asset.size,
                ) {
                    return Err(UpdateError::Integrity);
                }
            } else if size > 0 {
                file.set_len(0).map_err(|_| UpdateError::Storage)?;
                file.seek(SeekFrom::Start(0))
                    .map_err(|_| UpdateError::Storage)?;
                hash = Sha256::new();
                size = 0;
                progress(0, asset.size);
            }
            if response
                .content_length()
                .is_some_and(|length| length != asset.size - size)
            {
                return Err(UpdateError::Integrity);
            }
            while size < asset.size {
                let capacity = (asset.size - size).min(buffer.len() as u64) as usize;
                let count = response
                    .read(&mut buffer[..capacity])
                    .map_err(|_| UpdateError::Network)?;
                if count == 0 {
                    return Err(UpdateError::Network);
                }
                file.write_all(&buffer[..count])
                    .map_err(|_| UpdateError::Storage)?;
                hash.update(&buffer[..count]);
                size += count as u64;
                progress(size, asset.size);
            }
        }
        stage(TransferStage::Verifying);
        if format!("{:x}", hash.finalize()) != asset.sha256.to_ascii_lowercase() {
            return Err(UpdateError::Integrity);
        }
        file.sync_all().map_err(|_| UpdateError::Storage)?;
        Ok(())
    }
}
fn valid_range(value: Option<&str>, offset: u64, size: u64) -> bool {
    let Some((range, total)) = value
        .and_then(|s| s.strip_prefix("bytes "))
        .and_then(|s| s.split_once('/'))
    else {
        return false;
    };
    let Some((start, end)) = range.split_once('-') else {
        return false;
    };
    start.parse::<u64>().ok() == Some(offset)
        && end.parse::<u64>().ok() == size.checked_sub(1)
        && total.parse::<u64>().ok() == Some(size)
}
