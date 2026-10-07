//! Secrets at rest, tokens and password hashing.

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Encrypts service secrets with XChaCha20-Poly1305 under the master key.
#[derive(Clone)]
pub struct SecretBox {
    cipher: XChaCha20Poly1305,
}

impl std::fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretBox(••••)")
    }
}

impl SecretBox {
    pub fn new(key: &[u8; 32]) -> Self {
        Self {
            cipher: XChaCha20Poly1305::new(key.into()),
        }
    }

    /// A box with a random key (demo mode and tests).
    pub fn ephemeral() -> Self {
        let mut key = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        Self::new(&key)
    }

    /// Encrypt `plaintext`; `context` (e.g. "service-id/secret-name") is bound as associated data
    /// so a ciphertext can't be moved to another slot. Output: base64(nonce || ciphertext).
    pub fn seal(&self, context: &str, plaintext: &str) -> String {
        let mut nonce = [0u8; 24];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ciphertext = self
            .cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: context.as_bytes(),
                },
            )
            .expect("encryption cannot fail for in-memory buffers");
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ciphertext);
        STANDARD.encode(out)
    }

    pub fn open(&self, context: &str, sealed: &str) -> anyhow::Result<String> {
        let bytes = STANDARD.decode(sealed)?;
        anyhow::ensure!(bytes.len() > 24, "sealed value too short");
        let (nonce, ciphertext) = bytes.split_at(24);
        let plaintext = self
            .cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: context.as_bytes(),
                },
            )
            .map_err(|_| anyhow::anyhow!("cannot decrypt secret (wrong QUORUM_SECRET_KEY?)"))?;
        Ok(String::from_utf8(plaintext)?)
    }
}

/// A random URL-safe token with `bytes` bytes of entropy and an optional prefix.
pub fn random_token(prefix: &str, bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    format!("{prefix}{}", URL_SAFE_NO_PAD.encode(buf))
}

/// A human-typeable code from an unambiguous alphabet (no 0/O, 1/I/L).
pub fn random_code(len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut buf = vec![0u8; len];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    buf.iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect()
}

pub fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let salt = SaltString::encode_b64(&salt).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Constant-time comparison for equal-length byte strings.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_open_round_trip() {
        let sb = SecretBox::ephemeral();
        let sealed = sb.seal("svc/token", "hunter2");
        assert_ne!(sealed, "hunter2");
        assert_eq!(sb.open("svc/token", &sealed).unwrap(), "hunter2");
    }

    #[test]
    fn sealed_value_is_bound_to_context_and_key() {
        let sb = SecretBox::ephemeral();
        let sealed = sb.seal("a/token", "x");
        assert!(sb.open("b/token", &sealed).is_err());
        assert!(SecretBox::ephemeral().open("a/token", &sealed).is_err());
    }

    #[test]
    fn password_hash_verifies() {
        let hash = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &hash));
        assert!(!verify_password("wrong", &hash));
        assert!(!verify_password("x", "not a hash"));
    }

    #[test]
    fn codes_use_unambiguous_alphabet() {
        let code = random_code(64);
        assert_eq!(code.len(), 64);
        assert!(!code.contains(['0', 'O', '1', 'I', 'L']));
    }
}
