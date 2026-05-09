// Both sha1 and sha2 re-export the same `digest::Digest` trait; one import covers both.
use sha1::Digest as _;
use std::path::Path;
use tokio::io::AsyncReadExt;
use crate::Result;

const CHUNK_SIZE: usize = 64 * 1024;

pub async fn sha1_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = sha1::Sha1::new();
    let mut buf = vec![0u8; CHUNK_SIZE];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; CHUNK_SIZE];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub async fn sha512_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = sha2::Sha512::new();
    let mut buf = vec![0u8; CHUNK_SIZE];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub async fn verify_sha1(path: &Path, expected: &str) -> Result<()> {
    let actual = sha1_file(path).await?;
    if actual != expected {
        return Err(crate::Error::HashMismatch {
            file: path.to_path_buf(),
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

pub async fn verify_sha512(path: &Path, expected: &str) -> Result<()> {
    let actual = sha512_file(path).await?;
    if actual != expected {
        return Err(crate::Error::HashMismatch {
            file: path.to_path_buf(),
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}
