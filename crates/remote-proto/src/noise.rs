//! Noise sessions between a phone (initiator) and a desktop host (responder).
//!
//! - Paired device reconnecting: `Noise_IK` (the phone knows the host key; the host checks the
//!   phone key against its device list *before* answering).
//! - First pairing: `Noise_IKpsk2` with the one-time secret from the pairing code. The host
//!   only knows the secret matched once the first transport message decrypts, so it must not
//!   register the device before that.
//!
//! Both bind the host identity into the prologue. Every connection runs a fresh handshake;
//! a [`Session`] must never outlive its connection.
//!
//! The first frame of a connection is `[tag][noise message 1]`; the reply is `[noise message 2]`.

use crate::{Error, Result};
use snow::{Builder, HandshakeState, TransportState};

const PATTERN_IK: &str = "Noise_IK_25519_ChaChaPoly_SHA256";
const PATTERN_PAIR: &str = "Noise_IKpsk2_25519_ChaChaPoly_SHA256";
const TAG_IK: u8 = 1;
const TAG_PAIR: u8 = 2;

/// Largest Noise message is 65535 bytes; 16 are the AEAD tag and 1 is our chunk flag.
const CHUNK: usize = 16 * 1024;
/// Upper bound for a reassembled application message.
pub const MAX_MESSAGE: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct Keypair {
    pub private: [u8; 32],
    pub public: [u8; 32],
}

impl std::fmt::Debug for Keypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Keypair(public={})", crate::hex(&self.public))
    }
}

pub fn generate_keypair() -> Result<Keypair> {
    let kp = Builder::new(PATTERN_IK.parse()?).generate_keypair()?;
    Ok(Keypair {
        private: kp
            .private
            .try_into()
            .map_err(|_| Error::Frame("key length"))?,
        public: kp
            .public
            .try_into()
            .map_err(|_| Error::Frame("key length"))?,
    })
}

fn prologue(host_public: &[u8; 32]) -> Vec<u8> {
    let mut p = b"petruterm/1".to_vec();
    p.extend_from_slice(host_public);
    p
}

pub enum Mode {
    /// Device already paired with the host.
    Reconnect,
    /// First contact, authorised by the one-time secret from the pairing code.
    Pair { secret: [u8; 32] },
}

/// Client side, waiting for the host's reply.
pub struct Initiator {
    hs: HandshakeState,
}

/// Start a handshake. Returns the state and the first frame to send to the host.
pub fn initiate(
    local: &Keypair,
    host_public: &[u8; 32],
    mode: Mode,
) -> Result<(Initiator, Vec<u8>)> {
    let (pattern, tag, secret) = match &mode {
        Mode::Reconnect => (PATTERN_IK, TAG_IK, None),
        Mode::Pair { secret } => (PATTERN_PAIR, TAG_PAIR, Some(*secret)),
    };
    let prologue = prologue(host_public);
    let mut builder = Builder::new(pattern.parse()?)
        .local_private_key(&local.private)?
        .remote_public_key(host_public)?
        .prologue(&prologue)?;
    if let Some(secret) = &secret {
        builder = builder.psk(2, secret)?;
    }
    let mut hs = builder.build_initiator()?;
    let mut buf = vec![0u8; 512];
    let n = hs.write_message(&[], &mut buf)?;
    let mut frame = Vec::with_capacity(n + 1);
    frame.push(tag);
    frame.extend_from_slice(&buf[..n]);
    Ok((Initiator { hs }, frame))
}

impl Initiator {
    /// Consume the host's reply and enter transport mode.
    pub fn finish(mut self, reply: &[u8]) -> Result<Session> {
        let mut buf = vec![0u8; 512];
        self.hs.read_message(reply, &mut buf)?;
        Session::new(self.hs.into_transport_mode()?)
    }
}

/// Host side, after reading the first frame and before answering.
pub struct Responder {
    hs: HandshakeState,
    remote_static: [u8; 32],
    pairing: bool,
}

/// Read the first frame of a connection. `pair_secret` is the currently valid pairing offer,
/// if any; a pairing attempt with no offer is refused.
pub fn respond_read(
    local: &Keypair,
    first_frame: &[u8],
    pair_secret: Option<[u8; 32]>,
) -> Result<Responder> {
    let (&tag, msg) = first_frame
        .split_first()
        .ok_or(Error::Frame("empty handshake"))?;
    let (pattern, secret) = match tag {
        TAG_IK => (PATTERN_IK, None),
        TAG_PAIR => (
            PATTERN_PAIR,
            Some(pair_secret.ok_or(Error::Frame("no pairing offer"))?),
        ),
        _ => return Err(Error::Frame("unknown handshake tag")),
    };
    let prologue = prologue(&local.public);
    let mut builder = Builder::new(pattern.parse()?)
        .local_private_key(&local.private)?
        .prologue(&prologue)?;
    if let Some(secret) = &secret {
        builder = builder.psk(2, secret)?;
    }
    let mut hs = builder.build_responder()?;
    let mut buf = vec![0u8; 512];
    hs.read_message(msg, &mut buf)?;
    let remote_static: [u8; 32] = hs
        .get_remote_static()
        .ok_or(Error::Frame("no remote static"))?
        .try_into()
        .map_err(|_| Error::Frame("key length"))?;
    Ok(Responder {
        hs,
        remote_static,
        pairing: secret.is_some(),
    })
}

impl Responder {
    /// The phone's static public key, authenticated by the handshake (for IK).
    pub fn remote_static(&self) -> [u8; 32] {
        self.remote_static
    }

    /// True for IKpsk2. The device is not proven to hold the secret until the first message
    /// from it decrypts.
    pub fn is_pairing(&self) -> bool {
        self.pairing
    }

    /// Answer and enter transport mode. Call only after authorising [`Self::remote_static`].
    pub fn reply(mut self) -> Result<(Vec<u8>, Session)> {
        let mut buf = vec![0u8; 512];
        let n = self.hs.write_message(&[], &mut buf)?;
        buf.truncate(n);
        Ok((buf, Session::new(self.hs.into_transport_mode()?)?))
    }
}

/// An established encrypted channel. Messages are chunked into Noise-sized frames.
pub struct Session {
    ts: TransportState,
    partial: Vec<u8>,
}

impl Session {
    fn new(ts: TransportState) -> Result<Self> {
        Ok(Self {
            ts,
            partial: Vec::new(),
        })
    }

    /// Encrypt one application message into one or more frames, to be sent in order.
    pub fn seal(&mut self, message: &[u8]) -> Result<Vec<Vec<u8>>> {
        if message.len() > MAX_MESSAGE {
            return Err(Error::TooLarge);
        }
        let mut frames = Vec::new();
        let mut chunks = message.chunks(CHUNK).peekable();
        if chunks.peek().is_none() {
            frames.push(self.seal_chunk(true, &[])?);
        }
        while let Some(chunk) = chunks.next() {
            frames.push(self.seal_chunk(chunks.peek().is_none(), chunk)?);
        }
        Ok(frames)
    }

    fn seal_chunk(&mut self, last: bool, data: &[u8]) -> Result<Vec<u8>> {
        let mut plain = Vec::with_capacity(data.len() + 1);
        plain.push(last as u8);
        plain.extend_from_slice(data);
        let mut out = vec![0u8; plain.len() + 16];
        let n = self.ts.write_message(&plain, &mut out)?;
        out.truncate(n);
        Ok(out)
    }

    /// Decrypt one frame. Returns the full message once its last chunk arrives. Any error
    /// means the session is unusable (nonce state diverged): drop the connection.
    pub fn open(&mut self, frame: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut plain = vec![0u8; frame.len()];
        let n = self.ts.read_message(frame, &mut plain)?;
        let (&flag, data) = plain[..n]
            .split_first()
            .ok_or(Error::Frame("empty chunk"))?;
        if self.partial.len() + data.len() > MAX_MESSAGE {
            return Err(Error::TooLarge);
        }
        self.partial.extend_from_slice(data);
        if flag & 1 == 1 {
            Ok(Some(std::mem::take(&mut self.partial)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (Keypair, Keypair) {
        (generate_keypair().unwrap(), generate_keypair().unwrap())
    }

    fn handshake(
        client: &Keypair,
        host: &Keypair,
        mode: Mode,
        offer: Option<[u8; 32]>,
    ) -> Result<(Session, Session, [u8; 32])> {
        let (init, first) = initiate(client, &host.public, mode)?;
        let resp = respond_read(host, &first, offer)?;
        let seen = resp.remote_static();
        let (reply, host_session) = resp.reply()?;
        Ok((init.finish(&reply)?, host_session, seen))
    }

    #[test]
    fn reconnect_round_trip_and_host_sees_client_key() {
        let (client, host) = pair();
        let (mut c, mut h, seen) = handshake(&client, &host, Mode::Reconnect, None).unwrap();
        assert_eq!(seen, client.public);
        let frames = c.seal(b"hello host").unwrap();
        assert_eq!(h.open(&frames[0]).unwrap().unwrap(), b"hello host");
        let frames = h.seal(b"hello phone").unwrap();
        assert_eq!(c.open(&frames[0]).unwrap().unwrap(), b"hello phone");
    }

    #[test]
    fn pairing_with_correct_secret_works() {
        let (client, host) = pair();
        let secret = crate::random32().unwrap();
        let (mut c, mut h, _) =
            handshake(&client, &host, Mode::Pair { secret }, Some(secret)).unwrap();
        let frames = c.seal(b"x").unwrap();
        assert_eq!(h.open(&frames[0]).unwrap().unwrap(), b"x");
    }

    #[test]
    fn pairing_with_wrong_secret_fails_at_first_message() {
        let (client, host) = pair();
        let good = crate::random32().unwrap();
        let bad = crate::random32().unwrap();
        // The client may not even finish: the reply is keyed with the host's secret.
        let (init, first) = initiate(&client, &host.public, Mode::Pair { secret: bad }).unwrap();
        let resp = respond_read(&host, &first, Some(good)).unwrap();
        let (reply, _) = resp.reply().unwrap();
        assert!(init.finish(&reply).is_err());
    }

    #[test]
    fn pairing_without_an_offer_is_refused() {
        let (client, host) = pair();
        let secret = crate::random32().unwrap();
        let (_, first) = initiate(&client, &host.public, Mode::Pair { secret }).unwrap();
        assert!(respond_read(&host, &first, None).is_err());
    }

    #[test]
    fn handshake_to_the_wrong_host_key_fails() {
        let (client, host) = pair();
        let other = generate_keypair().unwrap();
        let (_, first) = initiate(&client, &other.public, Mode::Reconnect).unwrap();
        assert!(respond_read(&host, &first, None).is_err());
    }

    #[test]
    fn large_messages_are_chunked_and_reassembled() {
        let (client, host) = pair();
        let (mut c, mut h, _) = handshake(&client, &host, Mode::Reconnect, None).unwrap();
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let frames = c.seal(&big).unwrap();
        assert!(frames.len() > 1);
        assert!(frames.iter().all(|f| f.len() <= 65535));
        let mut got = None;
        for f in &frames {
            got = h.open(f).unwrap();
        }
        assert_eq!(got.unwrap(), big);
    }

    #[test]
    fn empty_message_round_trips() {
        let (client, host) = pair();
        let (mut c, mut h, _) = handshake(&client, &host, Mode::Reconnect, None).unwrap();
        let frames = c.seal(&[]).unwrap();
        assert_eq!(h.open(&frames[0]).unwrap().unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn replayed_or_reordered_frames_are_rejected() {
        let (client, host) = pair();
        let (mut c, mut h, _) = handshake(&client, &host, Mode::Reconnect, None).unwrap();
        let a = c.seal(b"a").unwrap().remove(0);
        let b = c.seal(b"b").unwrap().remove(0);
        assert!(h.open(&b).is_err(), "out of order");
        let (mut c, mut h, _) = handshake(&client, &host, Mode::Reconnect, None).unwrap();
        let a2 = c.seal(b"a").unwrap().remove(0);
        h.open(&a2).unwrap();
        assert!(h.open(&a2).is_err(), "replay");
        let _ = a;
    }

    #[test]
    fn tampered_frames_are_rejected() {
        let (client, host) = pair();
        let (mut c, mut h, _) = handshake(&client, &host, Mode::Reconnect, None).unwrap();
        let mut f = c.seal(b"secret").unwrap().remove(0);
        f[3] ^= 1;
        assert!(h.open(&f).is_err());
    }

    #[test]
    fn oversized_message_is_refused() {
        let (client, host) = pair();
        let (mut c, _, _) = handshake(&client, &host, Mode::Reconnect, None).unwrap();
        assert!(c.seal(&vec![0u8; MAX_MESSAGE + 1]).is_err());
    }
}
