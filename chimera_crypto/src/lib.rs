use ring::{aead, agreement, rand};
use anyhow::{Result, anyhow};

pub struct ChimeraCrypto;

impl ChimeraCrypto {
    /// Generate an ephemeral X25519 key pair.
    pub fn generate_ephemeral_key() -> Result<(agreement::EphemeralPrivateKey, Vec<u8>)> {
        let rng = rand::SystemRandom::new();
        let private_key = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &rng)
            .map_err(|_| anyhow!("Failed to generate private key"))?;
        let public_key = private_key.compute_public_key()
            .map_err(|_| anyhow!("Failed to compute public key"))?
            .as_ref()
            .to_vec();
        Ok((private_key, public_key))
    }

    /// Derive a shared secret key from a private key and a peer's public key.
    pub fn derive_secret(
        private_key: agreement::EphemeralPrivateKey,
        peer_public_key: &[u8],
    ) -> Result<Vec<u8>> {
        let peer_public_key_alg = &agreement::X25519;
        let peer_public_key = agreement::UnparsedPublicKey::new(peer_public_key_alg, peer_public_key);

        agreement::agree_ephemeral(
            private_key,
            &peer_public_key,
            |key_material| Ok::<Vec<u8>, ring::error::Unspecified>(key_material.to_vec()),
        )
        .map_err(|_| anyhow!("Key agreement failed"))?
        .map_err(|_| anyhow!("KDF failed"))
    }
}

pub struct Cipher {
    key: aead::LessSafeKey,
}

impl Cipher {
    pub fn new(key_bytes: &[u8]) -> Result<Self> {
        let unbound_key = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, key_bytes)
            .map_err(|_| anyhow!("Invalid key"))?;
        let key = aead::LessSafeKey::new(unbound_key);
        Ok(Self { key })
    }

    pub fn encrypt(&self, nonce_val: u64, data: &mut Vec<u8>) -> Result<()> {
        let nonce = self.create_nonce(nonce_val);
        self.key.seal_in_place_append_tag(nonce, aead::Aad::empty(), data)
            .map_err(|_| anyhow!("Encryption failed"))?;
        Ok(())
    }

    pub fn decrypt(&self, nonce_val: u64, data: &mut Vec<u8>) -> Result<usize> {
        let nonce = self.create_nonce(nonce_val);
        let decrypted_data = self.key.open_in_place(nonce, aead::Aad::empty(), data)
            .map_err(|_| anyhow!("Decryption failed"))?;
        Ok(decrypted_data.len())
    }

    fn create_nonce(&self, seq: u64) -> aead::Nonce {
        // Create a 12-byte nonce (96-bits).
        // We use the sequence number and pad with zeros.
        let mut nonce_bytes = [0u8; 12];
        let seq_bytes = seq.to_le_bytes();
        nonce_bytes[0..8].copy_from_slice(&seq_bytes);
        aead::Nonce::assume_unique_for_key(nonce_bytes)
    }
}
