//! PetruTerm remote access protocol, shared by the desktop host, the relay and the Android
//! client (through JNI). No GUI dependencies: this crate must build for Android.
//!
//! Layers, bottom up:
//! - [`noise`]: authenticated, encrypted sessions (Noise IK for paired devices, IKpsk2 for
//!   pairing), plus chunking of large messages into Noise-sized frames.
//! - [`msg`]: application packets (control JSON, terminal data, input).
//! - [`pair`]: the pairing code shown by the desktop and entered or scanned on the phone.
//!
//! The relay that carries these frames is untrusted: it only ever sees Noise ciphertext.

pub mod msg;
pub mod noise;
pub mod pair;

/// Wire protocol version, sent in `Hello` / `Welcome`.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("noise: {0}")]
    Noise(#[from] snow::Error),
    #[error("malformed frame: {0}")]
    Frame(&'static str),
    #[error("message too large")]
    TooLarge,
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("bad pairing code: {0}")]
    Pairing(&'static str),
    #[error("randomness unavailable")]
    Random,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Lowercase hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Inverse of [`hex`]; `None` on bad length or digits.
pub fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Relay endpoint id of a host: the hex of its static public key.
pub fn endpoint_id(host_public: &[u8; 32]) -> String {
    hex(host_public)
}

/// 32 random bytes.
pub fn random32() -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    getrandom::fill(&mut out).map_err(|_| Error::Random)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let bytes = [0u8, 1, 0xab, 0xff];
        assert_eq!(unhex::<4>(&hex(&bytes)), Some(bytes));
        assert_eq!(unhex::<4>("zz"), None);
        assert_eq!(unhex::<4>("0001abfg"), None);
        assert_eq!(unhex::<2>("0001abff"), None);
    }
}
