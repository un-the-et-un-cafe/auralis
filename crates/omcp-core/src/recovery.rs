//! Recovery phrase (M5, experimental).
//!
//! A 24-word phrase encodes 256 bits of entropy using the BIP39 English
//! wordlist and checksum (no BIP39 passphrase / PBKDF2 stretching — the
//! entropy itself is the identity master seed fed to
//! [`crate::StaticIdentity::from_seed`]). This keeps the mapping simple and
//! reversible; the phrase is the only secret needed to restore an identity.
//!
//! Not yet specified in PROTOCOL.md — treat as an app-level convention for
//! the v0.1 beta.

use bip39::{Language, Mnemonic};
use zeroize::Zeroizing;

use crate::error::{OmcpError, Result};

/// Number of words in a recovery phrase.
pub const RECOVERY_WORDS: usize = 24;

/// Fresh random 32-byte master seed.
pub fn generate_seed() -> Result<Zeroizing<[u8; 32]>> {
    let mut s = Zeroizing::new([0u8; 32]);
    getrandom::getrandom(s.as_mut()).map_err(|_| OmcpError::Crypto("getrandom"))?;
    Ok(s)
}

/// Encode a master seed as a 24-word phrase (space separated, lowercase).
pub fn seed_to_phrase(seed: &[u8; 32]) -> Result<Zeroizing<String>> {
    let m = Mnemonic::from_entropy_in(Language::English, seed)
        .map_err(|_| OmcpError::Crypto("mnemonic"))?;
    Ok(Zeroizing::new(m.to_string()))
}

/// Decode a phrase (whitespace/case tolerant) back into the master seed.
pub fn phrase_to_seed(phrase: &str) -> Result<Zeroizing<[u8; 32]>> {
    let norm = phrase
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ");
    let m = Mnemonic::parse_in_normalized(Language::English, &norm)
        .map_err(|_| OmcpError::Crypto("invalid recovery phrase"))?;
    if m.word_count() != RECOVERY_WORDS {
        return Err(OmcpError::Crypto("recovery phrase must be 24 words"));
    }
    let (ent, len) = m.to_entropy_array();
    if len != 32 {
        return Err(OmcpError::Crypto("recovery phrase entropy"));
    }
    let mut out = Zeroizing::new([0u8; 32]);
    out.copy_from_slice(&ent[..32]);
    Ok(out)
}

/// True if `word` is in the recovery word list.
pub fn is_recovery_word(word: &str) -> bool {
    Language::English.find_word(&word.to_lowercase()).is_some()
}

/// Up to `max` word-list entries starting with `prefix` (autocomplete chips).
pub fn suggest_words(prefix: &str, max: usize) -> Vec<&'static str> {
    let p = prefix.trim().to_lowercase();
    if p.is_empty() {
        return Vec::new();
    }
    Language::English
        .words_by_prefix(&p)
        .iter()
        .take(max)
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StaticIdentity;

    #[test]
    fn roundtrip_24_words() {
        let seed = generate_seed().unwrap();
        let phrase = seed_to_phrase(&seed).unwrap();
        assert_eq!(phrase.split(' ').count(), 24);
        let back = phrase_to_seed(&phrase.to_uppercase()).unwrap();
        assert_eq!(*back, *seed);
    }

    #[test]
    fn known_vector_all_zero() {
        // BIP39 test vector: 256-bit zero entropy.
        let p = seed_to_phrase(&[0u8; 32]).unwrap();
        assert!(p.starts_with("abandon abandon"));
        assert!(p.ends_with("art"));
    }

    #[test]
    fn rejects_bad_checksum_and_length() {
        let mut words: Vec<String> = seed_to_phrase(&[7u8; 32])
            .unwrap()
            .split(' ')
            .map(String::from)
            .collect();
        words[23] = if words[23] == "zoo" {
            "abandon".into()
        } else {
            "zoo".into()
        };
        assert!(phrase_to_seed(&words.join(" ")).is_err());
        assert!(phrase_to_seed("abandon abandon about").is_err());
    }

    #[test]
    fn word_helpers() {
        assert!(is_recovery_word("Abandon"));
        assert!(!is_recovery_word("hopla"));
        assert_eq!(suggest_words("foss", 3), vec!["fossil"]);
        assert!(suggest_words("a", 3).len() == 3);
        assert!(suggest_words("", 3).is_empty());
    }

    #[test]
    fn identity_from_seed_is_deterministic() {
        let a = StaticIdentity::from_seed(&[9u8; 32]).unwrap();
        let b = StaticIdentity::from_seed(&[9u8; 32]).unwrap();
        let c = StaticIdentity::from_seed(&[8u8; 32]).unwrap();
        assert_eq!(a.dh_public(), b.dh_public());
        assert_eq!(a.kem_public(), b.kem_public());
        assert_ne!(a.dh_public(), c.dh_public());
    }
}
