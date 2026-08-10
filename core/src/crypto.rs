use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, KeyInit}};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use rand::RngCore;

pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 12;

pub fn generate_salt_and_nonce() -> ([u8; SALT_LEN], [u8; NONCE_LEN]) {
    let mut rng = rand::thread_rng();
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    rng.fill_bytes(&mut salt);
    rng.fill_bytes(&mut nonce);
    (salt, nonce)
}

pub fn derive_key(password: &str, salt: &[u8; SALT_LEN]) -> Key<Aes256Gcm> {
    let mut key_bytes = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, 600_000, &mut key_bytes);
    Key::<Aes256Gcm>::from(key_bytes)
}

pub fn encrypt_chunk(
    key: &Key<Aes256Gcm>,
    base_nonce: &[u8; NONCE_LEN],
    chunk_index: u64,
    data: &[u8],
) -> Result<Vec<u8>, std::io::Error> {
    let cipher = Aes256Gcm::new(key);
    
    // Construct nonce by combining first 4 bytes of base_nonce with 8-byte chunk index
    // This strictly guarantees no nonce reuse for a single encryption session.
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes[0..4].copy_from_slice(&base_nonce[0..4]);
    nonce_bytes[4..12].copy_from_slice(&chunk_index.to_be_bytes());
    
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    cipher.encrypt(nonce, data)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Encryption failed"))
}

pub fn decrypt_chunk(
    key: &Key<Aes256Gcm>,
    base_nonce: &[u8; NONCE_LEN],
    chunk_index: u64,
    data: &[u8],
) -> Result<Vec<u8>, std::io::Error> {
    let cipher = Aes256Gcm::new(key);
    
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes[0..4].copy_from_slice(&base_nonce[0..4]);
    nonce_bytes[4..12].copy_from_slice(&chunk_index.to_be_bytes());

    let nonce = Nonce::from_slice(&nonce_bytes);
    
    cipher.decrypt(nonce, data)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Decryption failed (Wrong password or corrupted data)"))
}
