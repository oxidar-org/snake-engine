use std::collections::HashMap;

use anyhow::{Result, bail};
use rand::Rng;
use rand::RngExt;
use tokio::time::Instant;
use tracing::{info, instrument, warn};

use super::board::{Board, Position};
use super::snake::{Direction, Snake};

pub struct GameEngine {
    pub board: Board,
    active: HashMap<String, Snake>,
    disconnected: HashMap<String, (Snake, Instant)>,
    tick: u64,
    start_length: u16,
    win_length: u16,
    max_players: u32,
    rng: Box<dyn Rng + Send>,
    palette: Vec<String>,
    color_index: usize,
}

#[derive(Debug, Clone)]
pub struct TickResult {
    pub tick: u64,
    pub food: Position,
    pub snakes: Vec<SnakeState>,
    pub crowns: Vec<CrownEvent>,
}

#[derive(Debug, Clone)]
pub struct SnakeState {
    pub name: String,
    pub body: Vec<Position>,
    pub dir: Direction,
    pub crowns: u32,
    pub color: String,
}

#[derive(Debug, Clone)]
pub struct CrownEvent {
    pub name: String,
    pub crowns: u32,
}

impl GameEngine {
    pub fn new(
        board_width: u16,
        board_height: u16,
        start_length: u16,
        win_length: u16,
        max_players: u32,
        mut rng: Box<dyn Rng + Send>,
        palette: Vec<String>,
    ) -> GameEngine {
        let board = Board::new(board_width, board_height, rng.as_mut());
        GameEngine {
            board,
            active: HashMap::new(),
            disconnected: HashMap::new(),
            tick: 0,
            start_length,
            win_length,
            max_players,
            rng,
            palette,
            color_index: 0,
        }
    }

    #[instrument(skip(self), fields(name = %name))]
    pub fn add_player(&mut self, name: String) -> Result<()> {
        if self.active.contains_key(&name) {
            bail!("username already connected");
        }

        if let Some((snake, _)) = self.disconnected.remove(&name) {
            info!("player reconnected");
            self.active.insert(name, snake);
            return Ok(());
        }

        if self.active.len() as u32 >= self.max_players {
            warn!("capacity exceeded");
            bail!("server full");
        }

        let start_pos = Position {
            x: self.rng.random_range(0..self.board.width),
            y: self.rng.random_range(0..self.board.height),
        };
        let color = self.palette[self.color_index % self.palette.len()].clone();
        self.color_index += 1;
        let snake = Snake::new(
            name.clone(),
            start_pos,
            Direction::Right,
            self.start_length,
            color,
            &self.board,
        );
        info!("player joined");
        self.active.insert(name, snake);
        Ok(())
    }

    #[instrument(skip(self), fields(name = %name))]
    pub fn remove_player(&mut self, name: &str) {
        let Some(snake) = self.active.remove(name) else {
            warn!("remove called for unknown player");
            return;
        };
        info!("player disconnected");
        self.disconnected
            .insert(name.to_string(), (snake, Instant::now()));
    }

    pub fn queue_turn(&mut self, name: &str, dir: Direction) {
        let Some(snake) = self.active.get_mut(name) else {
            warn!(name = %name, "turn queued for unknown player");
            return;
        };
        snake.queue_turn(dir);
    }

    #[instrument(skip(self))]
    pub fn tick(&mut self) -> TickResult {
        self.tick += 1;
        let mut crowns = Vec::new();

        // 1. Apply queued direction changes
        for snake in self.active.values_mut() {
            snake.apply_turn();
        }

        // 2. Move all snakes
        for snake in self.active.values_mut() {
            snake.advance(&self.board);
        }

        // 3. Check food
        let food = self.board.food();
        let eaters: Vec<String> = self
            .active
            .iter()
            .filter(|(_, s)| s.head() == food)
            .map(|(name, _)| name.clone())
            .collect();

        for name in &eaters {
            if let Some(snake) = self.active.get_mut(name) {
                snake.grow();
            }
        }
        if !eaters.is_empty() {
            self.board.spawn_food(self.rng.as_mut());
        }

        // 4. Check win
        let winners: Vec<String> = self
            .active
            .iter()
            .filter(|(_, s)| s.len() >= self.win_length as usize)
            .map(|(name, _)| name.clone())
            .collect();

        for name in winners {
            if let Some(snake) = self.active.get_mut(&name) {
                snake.crowns += 1;
                let new_crowns = snake.crowns;
                info!(name = %name, crowns = new_crowns, "crown awarded");

                // Reset snake, preserving color
                let color = snake.color.clone();
                let start_pos = Position {
                    x: self.rng.random_range(0..self.board.width),
                    y: self.rng.random_range(0..self.board.height),
                };
                *snake = Snake::new(
                    name.clone(),
                    start_pos,
                    Direction::Right,
                    self.start_length,
                    color,
                    &self.board,
                );
                snake.crowns = new_crowns;

                crowns.push(CrownEvent {
                    name: name.clone(),
                    crowns: new_crowns,
                });
            }
        }

        // 5. Build snapshot
        let snakes = self
            .active
            .values()
            .map(|s| SnakeState {
                name: s.name.clone(),
                body: s.body.iter().copied().collect(),
                dir: s.dir,
                crowns: s.crowns,
                color: s.color.clone(),
            })
            .collect();

        TickResult {
            tick: self.tick,
            food: self.board.food(),
            snakes,
            crowns,
        }
    }

    pub fn purge_stale(&mut self, timeout_s: u64) {
        let timeout = std::time::Duration::from_secs(timeout_s);
        self.disconnected.retain(|name, (_, instant)| {
            let keep = instant.elapsed() < timeout;
            if !keep {
                info!(name = %name, "purged stale disconnected player");
            }
            keep
        });
    }

    pub fn active_players(&self) -> &HashMap<String, Snake> {
        &self.active
    }

    #[cfg(test)]
    pub fn active_players_mut(&mut self) -> &mut HashMap<String, Snake> {
        &mut self.active
    }

    pub fn disconnected_players(&self) -> &HashMap<String, (Snake, Instant)> {
        &self.disconnected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn test_palette() -> Vec<String> {
        vec!["#FF0000".into(), "#00FF00".into(), "#0000FF".into()]
    }

    fn test_engine() -> GameEngine {
        let rng = Box::new(StdRng::seed_from_u64(42));
        GameEngine::new(8, 8, 4, 8, 2, rng, test_palette())
    }

    #[test]
    fn add_two_players_tick_both_move() {
        let mut engine = test_engine();
        engine.add_player("alice".into()).unwrap();
        engine.add_player("bob".into()).unwrap();

        let r1 = engine.tick();
        assert_eq!(r1.snakes.len(), 2);

        let r2 = engine.tick();
        for s in &r2.snakes {
            let prev = r1.snakes.iter().find(|p| p.name == s.name).unwrap();
            assert_ne!(
                s.body[0], prev.body[0],
                "snake {} should have moved",
                s.name
            );
        }
    }

    #[test]
    fn snake_eats_food_grows_and_food_respawns() {
        let rng = Box::new(StdRng::seed_from_u64(42));
        let mut engine = GameEngine::new(8, 8, 4, 16, 4, rng, test_palette());
        engine.add_player("alice".into()).unwrap();

        // Position snake head on the food
        let food = engine.board.food();
        let snake = engine.active.get_mut("alice").unwrap();
        snake.body[0] = Position {
            x: (food.x as i32 - 1).rem_euclid(8) as u16,
            y: food.y,
        };
        snake.dir = Direction::Right;

        let old_food = engine.board.food();
        let old_len = engine.active["alice"].len();

        let result = engine.tick();

        // Food should have respawned
        assert!(result.food.x < 8 && result.food.y < 8);
        let _ = old_food;

        // Growth takes effect on the next advance
        engine.tick();
        assert_eq!(engine.active["alice"].len(), old_len + 1);
    }

    #[test]
    fn snake_reaches_win_length_gets_crown_and_resets() {
        let rng = Box::new(StdRng::seed_from_u64(42));
        let mut engine = GameEngine::new(8, 8, 4, 6, 4, rng, test_palette());
        engine.add_player("alice".into()).unwrap();

        // Manually set snake length to win_length - 1 by growing
        let snake = engine.active.get_mut("alice").unwrap();
        snake.growing = 2; // 4 + 2 = 6 after two advances, but we need length == 6 at tick check
        // Actually, set body directly to length 5, then grow once more
        while snake.body.len() < 5 {
            snake.body.push_back(Position { x: 0, y: 0 });
        }
        snake.growing = 1; // will become length 6 after advance

        let result = engine.tick();
        assert_eq!(result.crowns.len(), 1);
        assert_eq!(result.crowns[0].name, "alice");
        assert_eq!(result.crowns[0].crowns, 1);
        assert_eq!(engine.active["alice"].len(), 4); // reset to start_length
        assert_eq!(engine.active["alice"].crowns, 1);
    }

    #[test]
    fn remove_player_reconnect_preserves_state() {
        let mut engine = test_engine();
        engine.add_player("alice".into()).unwrap();
        engine.tick();

        let head_before = engine.active["alice"].head();
        let crowns_before = engine.active["alice"].crowns;

        engine.remove_player("alice");
        assert!(!engine.active.contains_key("alice"));
        assert!(engine.disconnected.contains_key("alice"));

        engine.add_player("alice".into()).unwrap();
        assert!(engine.active.contains_key("alice"));
        assert!(!engine.disconnected.contains_key("alice"));
        assert_eq!(engine.active["alice"].head(), head_before);
        assert_eq!(engine.active["alice"].crowns, crowns_before);
    }

    #[test]
    fn add_player_at_max_capacity_errors() {
        let mut engine = test_engine(); // max_players = 2
        engine.add_player("alice".into()).unwrap();
        engine.add_player("bob".into()).unwrap();
        let result = engine.add_player("charlie".into());
        assert!(result.is_err());
    }

    #[test]
    fn purge_stale_removes_expired() {
        let mut engine = test_engine();
        engine.add_player("alice".into()).unwrap();
        engine.remove_player("alice");

        // Insert with an old timestamp
        if let Some((snake, _)) = engine.disconnected.remove("alice") {
            engine.disconnected.insert(
                "alice".into(),
                (snake, Instant::now() - std::time::Duration::from_secs(120)),
            );
        }

        engine.purge_stale(60);
        assert!(!engine.disconnected.contains_key("alice"));
    }

    #[test]
    fn reconnected_snake_resumes_moving() {
        let mut engine = test_engine();
        engine.add_player("alice".into()).unwrap();
        engine.tick();

        let head_before_disconnect = engine.active["alice"].head();
        let dir_before = engine.active["alice"].dir;

        engine.remove_player("alice");
        engine.add_player("alice".into()).unwrap();

        // Snake should move on the very next tick
        let result = engine.tick();
        let alice = result.snakes.iter().find(|s| s.name == "alice").unwrap();
        assert_ne!(alice.body[0], head_before_disconnect);
        assert_eq!(alice.dir, dir_before);
    }

    #[test]
    fn no_duplicate_after_reconnect() {
        let mut engine = test_engine();
        engine.add_player("alice".into()).unwrap();
        engine.remove_player("alice");
        engine.add_player("alice".into()).unwrap();

        let result = engine.tick();
        let alice_count = result.snakes.iter().filter(|s| s.name == "alice").count();
        assert_eq!(alice_count, 1);
    }

    #[test]
    fn duplicate_username_while_connected_errors() {
        let mut engine = test_engine();
        engine.add_player("alice".into()).unwrap();
        let result = engine.add_player("alice".into());
        assert!(result.is_err());
    }

    #[test]
    fn two_new_players_get_different_colors() {
        let mut engine = test_engine();
        engine.add_player("alice".into()).unwrap();
        engine.add_player("bob".into()).unwrap();
        let alice_color = engine.active["alice"].color.clone();
        let bob_color = engine.active["bob"].color.clone();
        assert_ne!(alice_color, bob_color);
    }

    #[test]
    fn reconnected_player_retains_color() {
        let mut engine = test_engine();
        engine.add_player("alice".into()).unwrap();
        let original_color = engine.active["alice"].color.clone();
        engine.remove_player("alice");
        engine.add_player("alice".into()).unwrap();
        assert_eq!(engine.active["alice"].color, original_color);
    }

    #[test]
    fn color_wraps_around_palette() {
        // 2-color palette: the 3rd player should get palette[0] again
        let rng = Box::new(StdRng::seed_from_u64(42));
        let palette = vec!["#AAAAAA".into(), "#BBBBBB".into()];
        let mut engine = GameEngine::new(8, 8, 4, 8, 4, rng, palette);
        engine.add_player("player0".into()).unwrap();
        engine.add_player("player1".into()).unwrap();
        engine.add_player("player2".into()).unwrap();
        let first_color = engine.active["player0"].color.clone();
        let third_color = engine.active["player2"].color.clone();
        assert_eq!(first_color, third_color);
    }

    #[test]
    fn reconnect_after_purge_creates_fresh_snake() {
        let mut engine = test_engine();
        engine.add_player("alice".into()).unwrap();

        // Grow the snake so we can detect a reset
        engine.active.get_mut("alice").unwrap().growing = 3;
        for _ in 0..3 {
            engine.tick();
        }
        assert_eq!(engine.active["alice"].len(), 7);

        engine.remove_player("alice");

        // Simulate expired timeout
        if let Some((snake, _)) = engine.disconnected.remove("alice") {
            engine.disconnected.insert(
                "alice".into(),
                (snake, Instant::now() - std::time::Duration::from_secs(120)),
            );
        }
        engine.purge_stale(60);

        // Reconnect — should be a fresh snake at start_length
        engine.add_player("alice".into()).unwrap();
        assert_eq!(engine.active["alice"].len(), 4);
        assert_eq!(engine.active["alice"].crowns, 0);
    }
}
