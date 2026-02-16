use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::game::engine::{CrownEvent, SnakeState, TickResult};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Join { username: String },
    Turn { dir: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    State {
        tick: u64,
        food: [u16; 2],
        snakes: Vec<SnakeData>,
    },
    Crown {
        name: String,
        crowns: u32,
    },
    Leaderboard {
        players: Vec<LeaderboardEntry>,
    },
    Error {
        msg: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnakeData {
    pub name: String,
    pub body: Vec<[u16; 2]>,
    pub dir: u8,
    pub crowns: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub name: String,
    pub crowns: u32,
    pub length: u16,
    pub alive: bool,
}

pub fn encode(msg: &ServerMessage) -> Vec<u8> {
    rmp_serde::to_vec_named(msg).expect("ServerMessage serialization should not fail")
}

pub fn decode(bytes: &[u8]) -> Result<ClientMessage> {
    Ok(rmp_serde::from_slice(bytes)?)
}

impl From<&TickResult> for ServerMessage {
    fn from(result: &TickResult) -> ServerMessage {
        ServerMessage::State {
            tick: result.tick,
            food: [result.food.x, result.food.y],
            snakes: result.snakes.iter().map(SnakeData::from).collect(),
        }
    }
}

impl From<&CrownEvent> for ServerMessage {
    fn from(event: &CrownEvent) -> ServerMessage {
        ServerMessage::Crown {
            name: event.name.clone(),
            crowns: event.crowns,
        }
    }
}

impl From<&SnakeState> for SnakeData {
    fn from(s: &SnakeState) -> SnakeData {
        SnakeData {
            name: s.name.clone(),
            body: s.body.iter().map(|p| [p.x, p.y]).collect(),
            dir: s.dir as u8,
            crowns: s.crowns,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_state() {
        let msg = ServerMessage::State {
            tick: 42,
            food: [10, 5],
            snakes: vec![SnakeData {
                name: "alice".into(),
                body: vec![[1, 2], [1, 3]],
                dir: 0,
                crowns: 1,
            }],
        };
        let bytes = encode(&msg);
        // Decode as generic msgpack to verify structure
        let val: rmp_serde::decode::Error =
            rmp_serde::from_slice::<ClientMessage>(&bytes).unwrap_err();
        // It shouldn't decode as ClientMessage (different tag)
        assert!(!format!("{val}").is_empty());

        // But it should round-trip as ServerMessage
        let decoded: ServerMessage = rmp_serde::from_slice(&bytes).unwrap();
        match decoded {
            ServerMessage::State { tick, food, snakes } => {
                assert_eq!(tick, 42);
                assert_eq!(food, [10, 5]);
                assert_eq!(snakes.len(), 1);
                assert_eq!(snakes[0].name, "alice");
                assert_eq!(snakes[0].crowns, 1);
            }
            _ => panic!("expected State"),
        }
    }

    #[test]
    fn decode_valid_join() {
        let msg = ClientMessage::Join {
            username: "bob".into(),
        };
        let bytes = rmp_serde::to_vec_named(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        match decoded {
            ClientMessage::Join { username } => assert_eq!(username, "bob"),
            _ => panic!("expected Join"),
        }
    }

    #[test]
    fn decode_valid_turn() {
        let msg = ClientMessage::Turn { dir: 2 };
        let bytes = rmp_serde::to_vec_named(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        match decoded {
            ClientMessage::Turn { dir } => assert_eq!(dir, 2),
            _ => panic!("expected Turn"),
        }
    }

    #[test]
    fn decode_invalid_bytes() {
        let result = decode(&[0xFF, 0x00, 0x42]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_empty_bytes() {
        let result = decode(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_turn_with_invalid_dir_still_decodes() {
        // dir=5 is valid msgpack (u8), just not a valid Direction — handled at engine level
        let msg = ClientMessage::Turn { dir: 5 };
        let bytes = rmp_serde::to_vec_named(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        match decoded {
            ClientMessage::Turn { dir } => assert_eq!(dir, 5),
            _ => panic!("expected Turn"),
        }
    }

    #[test]
    fn decode_join_with_empty_username_still_decodes() {
        // Empty username is valid msgpack — validated at server level
        let msg = ClientMessage::Join {
            username: "".into(),
        };
        let bytes = rmp_serde::to_vec_named(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        match decoded {
            ClientMessage::Join { username } => assert!(username.is_empty()),
            _ => panic!("expected Join"),
        }
    }

    #[test]
    fn decode_server_message_as_client_fails() {
        let msg = ServerMessage::Error { msg: "test".into() };
        let bytes = encode(&msg);
        let result = decode(&bytes);
        assert!(result.is_err());
    }
}
