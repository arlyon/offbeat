//! Offline-verifiable festival certificates.
//!
//! Trust is rooted in a single **admin root** key (the MainDO's signing key),
//! whose public key is *pinned* in the app. The root signs a certificate
//! binding `festival_id → festival signing pubkey`. A client verifies that cert
//! against the pinned root before trusting a festival's key — so a cert relayed
//! peer-to-peer or replayed from cache is verifiable **without reaching any
//! server**. This is what lets a peer trust a festival diff relayed by another
//! peer (the gap left open in the ALPN catch-up phase).
//!
//! The signed payload format here is the single source of truth; the server /
//! admin tooling that issues certs MUST sign these exact bytes.

use ed25519_dalek::SigningKey;

use crate::signing;

/// Domain-separation tag so a festival cert signature can never be confused
/// with any other Ed25519 signature the admin root produces.
const CERT_DOMAIN: &[u8] = b"offbeat-festival-cert/v1";

/// Canonical bytes the admin root signs: domain tag, then NUL-delimited
/// `festival_id` and the 32-byte festival pubkey. NUL delimiting prevents
/// `("ab", "c")` and `("a", "bc")` from colliding.
fn cert_payload(festival_id: &str, festival_pubkey: &[u8; 32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(CERT_DOMAIN.len() + festival_id.len() + 34);
    buf.extend_from_slice(CERT_DOMAIN);
    buf.push(0);
    buf.extend_from_slice(festival_id.as_bytes());
    buf.push(0);
    buf.extend_from_slice(festival_pubkey);
    buf
}

/// A certificate binding a festival to its signing key, signed by the admin
/// root. Self-contained and offline-verifiable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FestivalCert {
    pub festival_id: String,
    pub festival_pubkey: [u8; 32],
    /// Ed25519 signature over [`cert_payload`] by the admin root key.
    pub signature: Vec<u8>,
}

impl FestivalCert {
    /// Verify this cert against the pinned admin root public key. Returns the
    /// trusted festival pubkey on success, or `None` if the signature doesn't
    /// chain to the pinned root.
    #[must_use]
    pub fn verify(&self, admin_root_pubkey: &[u8; 32]) -> Option<[u8; 32]> {
        let payload = cert_payload(&self.festival_id, &self.festival_pubkey);
        if signing::verify(admin_root_pubkey, &payload, &self.signature) {
            Some(self.festival_pubkey)
        } else {
            None
        }
    }
}

/// Issue (sign) a festival cert with the admin root signing key. Lives here so
/// the payload format stays identical on the signing and verifying sides; used
/// by Rust admin tooling and tests (the server signs the same bytes in TS).
#[must_use]
pub fn issue_festival_cert(
    admin_root: &SigningKey,
    festival_id: &str,
    festival_pubkey: &[u8; 32],
) -> FestivalCert {
    let signature = signing::sign(admin_root, &cert_payload(festival_id, festival_pubkey));
    FestivalCert {
        festival_id: festival_id.to_string(),
        festival_pubkey: *festival_pubkey,
        signature,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keypair() -> (SigningKey, [u8; 32]) {
        let k = signing::generate_signing_key();
        let pk = k.verifying_key().to_bytes();
        (k, pk)
    }

    #[test]
    fn valid_cert_verifies_to_festival_key() {
        let (root, root_pk) = keypair();
        let (_fest_key, fest_pk) = keypair();
        let cert = issue_festival_cert(&root, "glastonbury", &fest_pk);
        assert_eq!(cert.verify(&root_pk), Some(fest_pk));
    }

    #[test]
    fn cert_from_wrong_root_is_rejected() {
        let (root, _root_pk) = keypair();
        let (_, attacker_pk) = keypair();
        let (_fest_key, fest_pk) = keypair();
        let cert = issue_festival_cert(&root, "glastonbury", &fest_pk);
        // Pinned to a different root → no trust.
        assert_eq!(cert.verify(&attacker_pk), None);
    }

    #[test]
    fn tampered_festival_id_is_rejected() {
        let (root, root_pk) = keypair();
        let (_, fest_pk) = keypair();
        let mut cert = issue_festival_cert(&root, "glastonbury", &fest_pk);
        cert.festival_id = "download".to_string();
        assert_eq!(cert.verify(&root_pk), None);
    }

    #[test]
    fn tampered_festival_pubkey_is_rejected() {
        let (root, root_pk) = keypair();
        let (_, fest_pk) = keypair();
        let (_, other_pk) = keypair();
        let mut cert = issue_festival_cert(&root, "glastonbury", &fest_pk);
        // Swapping in an attacker's festival key breaks the binding.
        cert.festival_pubkey = other_pk;
        assert_eq!(cert.verify(&root_pk), None);
    }

    #[test]
    fn festival_id_delimiting_prevents_collision() {
        let (root, root_pk) = keypair();
        let (_, pk) = keypair();
        // Certs for distinct ids must not share a signature.
        let a = issue_festival_cert(&root, "ab", &pk);
        let b = issue_festival_cert(&root, "a\u{0}b", &pk);
        assert_ne!(a.signature, b.signature);
        assert!(a.verify(&root_pk).is_some());
    }
}
