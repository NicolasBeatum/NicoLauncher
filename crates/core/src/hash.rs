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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    // SHA vectors:
    //   SHA-1("")   = da39a3ee5e6b4b0d3255bfef95601890afd80709
    //   SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    //   SHA-512("") = cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce
    //                 47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e

    fn write_temp(content: &[u8]) -> (tempfile::NamedTempFile, std::path::PathBuf) {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content).unwrap();
        let p = f.path().to_path_buf();
        (f, p)
    }

    #[tokio::test]
    async fn sha1_of_empty_file() {
        let (_f, path) = write_temp(b"");
        let hash = sha1_file(&path).await.unwrap();
        assert_eq!(hash, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[tokio::test]
    async fn sha256_of_empty_file() {
        let (_f, path) = write_temp(b"");
        let hash = sha256_file(&path).await.unwrap();
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[tokio::test]
    async fn sha512_of_empty_file() {
        let (_f, path) = write_temp(b"");
        let hash = sha512_file(&path).await.unwrap();
        assert_eq!(
            hash,
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
             47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
    }

    #[tokio::test]
    async fn sha512_of_hello_world() {
        // echo -n "hello world" | sha512sum
        // = 309ecc489c12d6eb4cc40f50c902f2b4d0ed77ee511a7c7a9bcd3ca86d4cd86f
        //   989dd35bc5ff499670da34255b45b0cfd830e81f605dcf7dc5542e93ae9cd76f
        let (_f, path) = write_temp(b"hello world");
        let hash = sha512_file(&path).await.unwrap();
        assert_eq!(
            hash,
            "309ecc489c12d6eb4cc40f50c902f2b4d0ed77ee511a7c7a9bcd3ca86d4cd86f\
             989dd35bc5ff499670da34255b45b0cfd830e81f605dcf7dc5542e93ae9cd76f"
        );
    }

    #[tokio::test]
    async fn verify_sha512_passes_correct_hash() {
        let (_f, path) = write_temp(b"");
        let empty_hash = "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
                          47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e";
        assert!(verify_sha512(&path, empty_hash).await.is_ok());
    }

    #[tokio::test]
    async fn verify_sha512_fails_wrong_hash() {
        let (_f, path) = write_temp(b"");
        let result = verify_sha512(&path, "deadbeef").await;
        assert!(result.is_err());
        // Should be a HashMismatch, not an IO error
        let err = result.unwrap_err().to_string();
        assert!(err.contains("hash") || err.contains("Hash") || err.contains("mismatch"),
                "unexpected error: {err}");
    }

    #[tokio::test]
    async fn sha1_of_same_content_is_deterministic() {
        let (_f1, p1) = write_temp(b"deterministic content");
        let (_f2, p2) = write_temp(b"deterministic content");
        let h1 = sha1_file(&p1).await.unwrap();
        let h2 = sha1_file(&p2).await.unwrap();
        assert_eq!(h1, h2);
    }
}
