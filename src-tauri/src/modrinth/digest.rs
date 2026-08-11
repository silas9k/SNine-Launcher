use super::{validation, ModrinthFile, VerifiedFileDigest};
use crate::error::{AppError, AppResult};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

pub struct DownloadVerifier {
    expected_size: u64,
    expected_sha512: String,
    expected_sha1: Option<String>,
    received: u64,
    sha512: Sha512,
    sha1: Sha1,
    sha256: Sha256,
}

impl DownloadVerifier {
    pub fn new(file: &ModrinthFile) -> AppResult<Self> {
        if file.size_bytes == 0 || file.size_bytes > validation::MAX_MODRINTH_FILE_SIZE_BYTES {
            return Err(AppError::coded("modrinth_file_size_invalid"));
        }
        validation::validate_sha512(&file.upstream_sha512)?;
        if let Some(sha1) = file.upstream_sha1.as_deref() {
            validation::validate_sha1(sha1)?;
        }
        Ok(Self {
            expected_size: file.size_bytes,
            expected_sha512: file.upstream_sha512.clone(),
            expected_sha1: file.upstream_sha1.clone(),
            received: 0,
            sha512: Sha512::new(),
            sha1: Sha1::new(),
            sha256: Sha256::new(),
        })
    }

    pub fn push(&mut self, bytes: &[u8]) -> AppResult<()> {
        let next = self
            .received
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| AppError::coded("modrinth_download_size_overflow"))?;
        if next > self.expected_size {
            return Err(AppError::coded("modrinth_download_size_mismatch"));
        }
        self.sha512.update(bytes);
        self.sha1.update(bytes);
        self.sha256.update(bytes);
        self.received = next;
        Ok(())
    }

    pub fn finish(self) -> AppResult<VerifiedFileDigest> {
        if self.received != self.expected_size {
            return Err(AppError::coded("modrinth_download_size_mismatch"));
        }
        let actual_sha512 = hex::encode(self.sha512.finalize());
        if actual_sha512 != self.expected_sha512 {
            return Err(AppError::coded("modrinth_download_sha512_mismatch"));
        }
        if let Some(expected_sha1) = self.expected_sha1 {
            let actual_sha1 = hex::encode(self.sha1.finalize());
            if actual_sha1 != expected_sha1 {
                return Err(AppError::coded("modrinth_download_sha1_mismatch"));
            }
        }
        Ok(VerifiedFileDigest {
            size_bytes: self.received,
            sha256: hex::encode(self.sha256.finalize()),
        })
    }
}

pub fn verify_file_bytes(file: &ModrinthFile, bytes: &[u8]) -> AppResult<VerifiedFileDigest> {
    let mut verifier = DownloadVerifier::new(file)?;
    verifier.push(bytes)?;
    verifier.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Url;

    const HELLO_SHA512: &str = "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043";
    const HELLO_SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";
    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn file(size: u64, sha512: &str, sha1: Option<&str>) -> ModrinthFile {
        ModrinthFile::new(
            "hello.jar".into(),
            size,
            true,
            sha512.into(),
            sha1.map(str::to_owned),
            None,
            Url::parse("https://cdn.modrinth.com/data/AABBCCDD/versions/1/hello.jar")
                .expect("fixture URL"),
        )
    }

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result.expect_err("expected error").descriptor().code
    }

    #[test]
    fn sha512_is_verified_before_launcher_sha256_is_emitted() {
        let digest = verify_file_bytes(&file(5, HELLO_SHA512, Some(HELLO_SHA1)), b"hello")
            .expect("verified fixture");
        assert_eq!(digest.size_bytes, 5);
        assert_eq!(digest.sha256, HELLO_SHA256);
    }

    #[test]
    fn verifier_supports_streaming_without_weakening_hash_requirements() {
        let file = file(5, HELLO_SHA512, None);
        let mut verifier = DownloadVerifier::new(&file).expect("verifier");
        verifier.push(b"he").expect("first chunk");
        verifier.push(b"llo").expect("second chunk");
        assert_eq!(verifier.finish().expect("finish").sha256, HELLO_SHA256);
    }

    #[test]
    fn size_or_upstream_hash_mismatch_produces_no_internal_digest() {
        assert_eq!(
            error_code(verify_file_bytes(
                &file(4, HELLO_SHA512, Some(HELLO_SHA1)),
                b"hello",
            )),
            "modrinth_download_size_mismatch"
        );
        assert_eq!(
            error_code(verify_file_bytes(
                &file(5, &"0".repeat(128), None),
                b"hello"
            )),
            "modrinth_download_sha512_mismatch"
        );
        assert_eq!(
            error_code(verify_file_bytes(
                &file(5, HELLO_SHA512, Some(&"0".repeat(40))),
                b"hello",
            )),
            "modrinth_download_sha1_mismatch"
        );
    }

    #[test]
    fn serialized_file_metadata_never_exposes_the_download_url() {
        let value = serde_json::to_value(file(5, HELLO_SHA512, Some(HELLO_SHA1)))
            .expect("serialize metadata");
        assert!(value.get("downloadUrl").is_none());
        assert!(!value.to_string().contains("cdn.modrinth.com"));
        assert_eq!(value["upstreamSha512"], HELLO_SHA512);
    }
}
