//! The pairing code: what the desktop shows (as text and QR) and the phone consumes.
//!
//! `petruterm1:` + base64url(no padding) of
//! `[version u8][host public key 32][secret 32][expires unix secs u64 BE][relay URL utf8]`.
//!
//! The secret is single use and short lived; the host enforces both. The code is sensitive
//! until it is used or expires.

use crate::{Error, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

const PREFIX: &str = "petruterm1:";
const VERSION: u8 = 1;
const HEADER: usize = 1 + 32 + 32 + 8;

#[derive(Clone, PartialEq, Eq)]
pub struct PairingCode {
    pub host_public: [u8; 32],
    pub secret: [u8; 32],
    pub expires_unix: u64,
    pub relay: String,
}

impl std::fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PairingCode(relay={}, secret=<hidden>)", self.relay)
    }
}

impl PairingCode {
    pub fn encode(&self) -> String {
        let mut raw = Vec::with_capacity(HEADER + self.relay.len());
        raw.push(VERSION);
        raw.extend_from_slice(&self.host_public);
        raw.extend_from_slice(&self.secret);
        raw.extend_from_slice(&self.expires_unix.to_be_bytes());
        raw.extend_from_slice(self.relay.as_bytes());
        format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(raw))
    }

    pub fn decode(code: &str) -> Result<Self> {
        let body = code
            .trim()
            .strip_prefix(PREFIX)
            .ok_or(Error::Pairing("missing petruterm1: prefix"))?;
        let raw = URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|_| Error::Pairing("not valid base64"))?;
        if raw.len() < HEADER || raw[0] != VERSION {
            return Err(Error::Pairing("unsupported or truncated code"));
        }
        let relay = String::from_utf8(raw[HEADER..].to_vec())
            .map_err(|_| Error::Pairing("relay url is not utf-8"))?;
        if !(relay.starts_with("ws://") || relay.starts_with("wss://")) {
            return Err(Error::Pairing("relay url must be ws:// or wss://"));
        }
        Ok(Self {
            host_public: raw[1..33].try_into().unwrap(),
            secret: raw[33..65].try_into().unwrap(),
            expires_unix: u64::from_be_bytes(raw[65..73].try_into().unwrap()),
            relay,
        })
    }

    pub fn is_expired(&self, now_unix: u64) -> bool {
        now_unix >= self.expires_unix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PairingCode {
        PairingCode {
            host_public: [7; 32],
            secret: [9; 32],
            expires_unix: 1_800_000_000,
            relay: "wss://relay.example.com/".into(),
        }
    }

    #[test]
    fn round_trip() {
        let code = sample();
        assert_eq!(PairingCode::decode(&code.encode()).unwrap(), code);
        assert_eq!(
            PairingCode::decode(&format!("  {}\n", code.encode())).unwrap(),
            code
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert!(PairingCode::decode("nope").is_err());
        assert!(PairingCode::decode("petruterm1:!!!").is_err());
        assert!(PairingCode::decode("petruterm1:AAAA").is_err());
        let mut c = sample();
        c.relay = "http://x".into();
        assert!(PairingCode::decode(&c.encode()).is_err());
    }

    #[test]
    fn expiry() {
        let c = sample();
        assert!(!c.is_expired(c.expires_unix - 1));
        assert!(c.is_expired(c.expires_unix));
    }

    #[test]
    fn debug_hides_the_secret() {
        assert!(!format!("{:?}", sample()).contains('9'));
    }
}
