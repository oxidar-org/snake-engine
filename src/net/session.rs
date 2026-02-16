use std::collections::HashMap;

use anyhow::{Result, bail};
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(u64);

#[derive(Debug, Clone)]
pub enum Session {
    Player { username: String },
    Spectator,
}

pub struct SessionManager {
    sessions: HashMap<SessionId, Session>,
    next_id: u64,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> SessionManager {
        SessionManager {
            sessions: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn connect(&mut self) -> SessionId {
        let id = SessionId(self.next_id);
        self.next_id += 1;
        self.sessions.insert(id, Session::Spectator);
        info!(session = id.0, "new connection (spectator)");
        id
    }

    pub fn promote(&mut self, id: SessionId, username: String) -> Result<()> {
        let username_taken = self.sessions.values().any(|s| match s {
            Session::Player { username: u } => u == &username,
            _ => false,
        });

        if username_taken {
            warn!(session = id.0, username = %username, "duplicate username");
            bail!("username already connected");
        }

        if let Some(session) = self.sessions.get_mut(&id) {
            *session = Session::Player {
                username: username.clone(),
            };
            info!(session = id.0, username = %username, "promoted to player");
            Ok(())
        } else {
            bail!("session not found");
        }
    }

    pub fn disconnect(&mut self, id: SessionId) -> Option<Session> {
        let session = self.sessions.remove(&id);
        if let Some(ref s) = session {
            info!(session = id.0, ?s, "disconnected");
        }
        session
    }

    pub fn get(&self, id: SessionId) -> Option<&Session> {
        self.sessions.get(&id)
    }

    #[allow(dead_code)]
    pub fn player_sessions(&self) -> impl Iterator<Item = (SessionId, &str)> {
        self.sessions.iter().filter_map(|(id, s)| match s {
            Session::Player { username } => Some((*id, username.as_str())),
            _ => None,
        })
    }

    #[allow(dead_code)]
    pub fn all_sessions(&self) -> impl Iterator<Item = SessionId> + '_ {
        self.sessions.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_is_spectator_promote_is_player_disconnect_removes() {
        let mut mgr = SessionManager::new();
        let id = mgr.connect();

        assert!(matches!(mgr.get(id), Some(Session::Spectator)));

        mgr.promote(id, "alice".into()).unwrap();
        match mgr.get(id) {
            Some(Session::Player { username }) => assert_eq!(username, "alice"),
            _ => panic!("expected Player"),
        }

        let session = mgr.disconnect(id);
        assert!(session.is_some());
        assert!(mgr.get(id).is_none());
    }

    #[test]
    fn promote_duplicate_username_errors() {
        let mut mgr = SessionManager::new();
        let id1 = mgr.connect();
        let id2 = mgr.connect();

        mgr.promote(id1, "alice".into()).unwrap();
        let result = mgr.promote(id2, "alice".into());
        assert!(result.is_err());
    }

    #[test]
    fn disconnect_player_then_promote_new_with_same_username() {
        let mut mgr = SessionManager::new();
        let id1 = mgr.connect();
        mgr.promote(id1, "alice".into()).unwrap();
        mgr.disconnect(id1);

        let id2 = mgr.connect();
        mgr.promote(id2, "alice".into()).unwrap();
        match mgr.get(id2) {
            Some(Session::Player { username }) => assert_eq!(username, "alice"),
            _ => panic!("expected Player"),
        }
    }

    #[test]
    fn spectator_is_not_player() {
        let mut mgr = SessionManager::new();
        let id = mgr.connect();

        // A spectator session should not be a Player variant
        assert!(matches!(mgr.get(id), Some(Session::Spectator)));

        // Confirm spectator is not listed in player_sessions
        assert_eq!(mgr.player_sessions().count(), 0);
    }
}
