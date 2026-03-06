use crate::game::engine::GameEngine;
use crate::net::protocol::LeaderboardEntry;

pub fn compute(engine: &GameEngine) -> Vec<LeaderboardEntry> {
    let active = engine.active_players().values().map(|s| LeaderboardEntry {
        name: s.name.clone(),
        crowns: s.crowns,
        length: s.len() as u16,
        alive: true,
        country: s.country.clone(),
    });

    let disconnected = engine
        .disconnected_players()
        .values()
        .map(|(s, _)| LeaderboardEntry {
            name: s.name.clone(),
            crowns: s.crowns,
            length: 0,
            alive: false,
            country: s.country.clone(),
        });

    let mut entries: Vec<_> = active.chain(disconnected).collect();
    entries.sort_by(|a, b| b.crowns.cmp(&a.crowns).then(b.length.cmp(&a.length)));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn test_engine() -> GameEngine {
        let rng = Box::new(StdRng::seed_from_u64(42));
        let palette = vec!["#FF0000".into(), "#00FF00".into(), "#0000FF".into()];
        GameEngine::new(8, 8, 4, 16, 32, rng, palette)
    }

    #[test]
    fn sorted_by_crowns_descending() {
        let mut engine = test_engine();
        engine.add_player("alice".into(), None).unwrap();
        engine.add_player("bob".into(), None).unwrap();
        engine.add_player("charlie".into(), None).unwrap();

        // Manually set crowns
        engine.active_players_mut().get_mut("alice").unwrap().crowns = 5;
        engine.active_players_mut().get_mut("bob").unwrap().crowns = 3;
        engine
            .active_players_mut()
            .get_mut("charlie")
            .unwrap()
            .crowns = 7;

        let lb = compute(&engine);
        assert_eq!(lb[0].name, "charlie");
        assert_eq!(lb[0].crowns, 7);
        assert_eq!(lb[1].name, "alice");
        assert_eq!(lb[2].name, "bob");
    }

    #[test]
    fn tie_in_crowns_uses_length_tiebreaker() {
        let mut engine = test_engine();
        engine.add_player("alice".into(), None).unwrap();
        engine.add_player("bob".into(), None).unwrap();

        engine.active_players_mut().get_mut("alice").unwrap().crowns = 3;
        engine.active_players_mut().get_mut("bob").unwrap().crowns = 3;
        // Grow bob so he's longer
        engine.active_players_mut().get_mut("bob").unwrap().growing = 4;
        for _ in 0..4 {
            engine.tick();
        }

        let lb = compute(&engine);
        assert_eq!(lb[0].name, "bob");
        assert_eq!(lb[1].name, "alice");
    }

    #[test]
    fn disconnected_shows_alive_false_and_length_zero() {
        let mut engine = test_engine();
        engine.add_player("alice".into(), None).unwrap();
        engine.add_player("bob".into(), None).unwrap();
        engine.remove_player("bob");

        let lb = compute(&engine);
        let alice = lb.iter().find(|e| e.name == "alice").unwrap();
        let bob = lb.iter().find(|e| e.name == "bob").unwrap();

        assert!(alice.alive);
        assert!(alice.length > 0);
        assert!(!bob.alive);
        assert_eq!(bob.length, 0);
    }
}
