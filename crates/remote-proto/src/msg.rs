//! Application packets carried inside a [`crate::noise::Session`].
//!
//! Packet layout (after reassembly):
//! - `0x01` + JSON: control ([`ClientMsg`] from the phone, [`HostMsg`] from the host)
//! - `0x02` + pane u64 BE + kind u8 + bytes: terminal data, host to phone
//! - `0x03` + pane u64 BE + bytes: keyboard input, phone to host

use crate::{Error, Result};
use serde::{Deserialize, Serialize};

const KIND_CONTROL: u8 = 1;
const KIND_DATA: u8 = 2;
const KIND_INPUT: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    /// ANSI that rebuilds the screen; always the first data packet after `Attached`.
    Snapshot,
    /// Raw PTY output following the snapshot.
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Client(ClientMsg),
    Host(HostMsg),
    Data {
        pane: u64,
        kind: DataKind,
        bytes: Vec<u8>,
    },
    Input {
        pane: u64,
        bytes: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    Hello {
        version: u32,
        name: String,
    },
    List,
    Attach {
        pane: u64,
    },
    Detach {
        pane: u64,
    },
    /// Fit the pane to the phone while attached; the host restores the desktop size on
    /// detach or disconnect.
    Resize {
        pane: u64,
        cols: u16,
        rows: u16,
    },
    Ping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum HostMsg {
    Welcome {
        version: u32,
        host: String,
        read_only: bool,
    },
    Panes {
        panes: Vec<PaneInfo>,
    },
    Attached {
        pane: u64,
        cols: u16,
        rows: u16,
    },
    Detached {
        pane: u64,
        reason: String,
    },
    /// An agent in `pane` changed state ("working", "needs-input", "done", "idle").
    Attention {
        pane: u64,
        state: String,
        message: String,
    },
    Error {
        message: String,
    },
    Pong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneInfo {
    pub uid: u64,
    pub title: String,
    pub workspace: String,
    pub cols: u16,
    pub rows: u16,
    pub state: Option<String>,
    pub unread: bool,
}

impl Packet {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        match self {
            Packet::Client(m) => {
                out.push(KIND_CONTROL);
                out.extend(serde_json::to_vec(m)?);
            }
            Packet::Host(m) => {
                out.push(KIND_CONTROL);
                out.extend(serde_json::to_vec(m)?);
            }
            Packet::Data { pane, kind, bytes } => {
                out.push(KIND_DATA);
                out.extend(pane.to_be_bytes());
                out.push(match kind {
                    DataKind::Snapshot => 1,
                    DataKind::Output => 2,
                });
                out.extend(bytes);
            }
            Packet::Input { pane, bytes } => {
                out.push(KIND_INPUT);
                out.extend(pane.to_be_bytes());
                out.extend(bytes);
            }
        }
        Ok(out)
    }

    /// Decode a packet sent by the phone (host side).
    pub fn decode_from_client(buf: &[u8]) -> Result<Packet> {
        match buf.split_first() {
            Some((&KIND_CONTROL, json)) => Ok(Packet::Client(serde_json::from_slice(json)?)),
            Some((&KIND_INPUT, rest)) => {
                let (pane, bytes) = split_pane(rest)?;
                Ok(Packet::Input {
                    pane,
                    bytes: bytes.to_vec(),
                })
            }
            _ => Err(Error::Frame("unexpected packet from client")),
        }
    }

    /// Decode a packet sent by the host (phone side).
    pub fn decode_from_host(buf: &[u8]) -> Result<Packet> {
        match buf.split_first() {
            Some((&KIND_CONTROL, json)) => Ok(Packet::Host(serde_json::from_slice(json)?)),
            Some((&KIND_DATA, rest)) => {
                let (pane, rest) = split_pane(rest)?;
                let (&kind, bytes) = rest.split_first().ok_or(Error::Frame("data kind"))?;
                let kind = match kind {
                    1 => DataKind::Snapshot,
                    2 => DataKind::Output,
                    _ => return Err(Error::Frame("data kind")),
                };
                Ok(Packet::Data {
                    pane,
                    kind,
                    bytes: bytes.to_vec(),
                })
            }
            _ => Err(Error::Frame("unexpected packet from host")),
        }
    }
}

fn split_pane(buf: &[u8]) -> Result<(u64, &[u8])> {
    let (head, rest) = buf
        .split_at_checked(8)
        .ok_or(Error::Frame("pane id truncated"))?;
    Ok((u64::from_be_bytes(head.try_into().unwrap()), rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_round_trips() {
        let p = Packet::Client(ClientMsg::Attach { pane: 7 });
        assert_eq!(Packet::decode_from_client(&p.encode().unwrap()).unwrap(), p);
        let p = Packet::Host(HostMsg::Attached {
            pane: 7,
            cols: 80,
            rows: 24,
        });
        assert_eq!(Packet::decode_from_host(&p.encode().unwrap()).unwrap(), p);
    }

    #[test]
    fn data_and_input_round_trip_with_binary_bytes() {
        let p = Packet::Data {
            pane: u64::MAX,
            kind: DataKind::Output,
            bytes: vec![0, 255, 27, 91],
        };
        assert_eq!(Packet::decode_from_host(&p.encode().unwrap()).unwrap(), p);
        let p = Packet::Input {
            pane: 3,
            bytes: b"ls\r".to_vec(),
        };
        assert_eq!(Packet::decode_from_client(&p.encode().unwrap()).unwrap(), p);
    }

    #[test]
    fn direction_is_enforced() {
        let data = Packet::Data {
            pane: 1,
            kind: DataKind::Output,
            bytes: vec![],
        }
        .encode()
        .unwrap();
        assert!(Packet::decode_from_client(&data).is_err());
        let input = Packet::Input {
            pane: 1,
            bytes: vec![],
        }
        .encode()
        .unwrap();
        assert!(Packet::decode_from_host(&input).is_err());
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(Packet::decode_from_client(&[]).is_err());
        assert!(Packet::decode_from_client(&[3, 1, 2]).is_err());
        assert!(Packet::decode_from_host(&[2, 0, 0]).is_err());
        assert!(Packet::decode_from_client(&[1, b'{']).is_err());
    }
}
