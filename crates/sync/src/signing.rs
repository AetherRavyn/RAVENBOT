//! Bundle signing and verification

use ed25519_dalek::{SigningKey, Signer, Verifier};
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};

/// Bundle signer for export/import
pub struct BundleSigner {
    signing_key: SigningKey,
}

impl BundleSigner {
    pub fn new() -> Self {
        // In production, load from OS keychain
        // For now, generate a key (in production, persist this)
        let signing_key = Self::get_or_create_keypair();
        Self { signing_key }
    }

    /// Build a signer from a persisted 32-byte seed
    pub fn from_bytes(seed: [u8; 32]) -> Self {
        Self { signing_key: SigningKey::from_bytes(&seed) }
    }

    /// Get or create keypair (in production, use OS keychain)
    fn get_or_create_keypair() -> SigningKey {
        // In production:
        // 1. Check keyring for existing key
        // 2. If not found, generate new key
        // 3. Store in keyring
        
        // For demo, use a deterministic key
        let seed = [1u8; 32]; // In production, generate random
        SigningKey::from_bytes(&seed)
    }

    /// Get private key bytes
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Get public key bytes
    pub fn public_key(&self) -> Vec<u8> {
        self.signing_key.verifying_key().to_bytes().to_vec()
    }

    /// Sign data
    pub fn sign(&self, data: &[u8]) -> String {
        let signature = self.signing_key.sign(data);
        general_purpose::STANDARD.encode(signature.to_bytes())
    }

    /// Verify signature
    pub fn verify(&self, data: &[u8], signature_str: &str) -> bool {
        let signature_bytes = match general_purpose::STANDARD.decode(signature_str) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        let signature = match ed25519_dalek::Signature::from_slice(&signature_bytes) {
            Ok(sig) => sig,
            Err(_) => return false,
        };

        self.signing_key.verifying_key().verify(data, &signature).is_ok()
    }

    /// Sign a bundle
    pub fn sign_bundle(&self, bundle_data: &[u8]) -> String {
        self.sign(bundle_data)
    }

    /// Verify a bundle signature
    pub fn verify_bundle(&self, bundle_data: &[u8], signature: &str) -> bool {
        self.verify(bundle_data, signature)
    }

    /// Public key as base64 (embedded in exported bundles for TOFU verification)
    pub fn public_key_b64(&self) -> String {
        general_purpose::STANDARD.encode(self.public_key())
    }

    /// Verify a signature against an *arbitrary* public key (base64) —
    /// used on import to verify foreign bundles before trusting their key.
    pub fn verify_with(data: &[u8], signature_str: &str, pubkey_b64: &str) -> bool {
        let Ok(pubkey_bytes) = general_purpose::STANDARD.decode(pubkey_b64) else {
            return false;
        };
        let Ok(pubkey_arr) = <[u8; 32]>::try_from(pubkey_bytes.as_slice()) else {
            return false;
        };
        let Ok(verifying_key) = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_arr) else {
            return false;
        };
        let Ok(signature_bytes) = general_purpose::STANDARD.decode(signature_str) else {
            return false;
        };
        let Ok(signature) = ed25519_dalek::Signature::from_slice(&signature_bytes) else {
            return false;
        };
        verifying_key.verify(data, &signature).is_ok()
    }

    /// Fingerprint for a trusted-keys table: the base64 pubkey itself
    /// (stable, unique, already encoded)
    pub fn fingerprint(pubkey_b64: &str) -> String {
        pubkey_b64.to_string()
    }
}

impl Default for BundleSigner {
    fn default() -> Self {
        Self::new()
    }
}

/// Bundle signature format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleSignature {
    /// Signature bytes (base64)
    pub signature: String,
    /// Public key of signer
    pub public_key: String,
    /// Algorithm used
    pub algorithm: String,
}

impl BundleSignature {
    pub fn new(signature: String, public_key: Vec<u8>) -> Self {
        Self {
            signature,
            public_key: general_purpose::STANDARD.encode(&public_key),
            algorithm: "ed25519".to_string(),
        }
    }
}
