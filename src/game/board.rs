use rand::Rng;
use rand::RngExt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

pub struct Board {
    pub width: u16,
    pub height: u16,
    food: Position,
}

impl Board {
    pub fn new(width: u16, height: u16, rng: &mut (impl Rng + ?Sized)) -> Board {
        let food = Position {
            x: rng.random_range(0..width),
            y: rng.random_range(0..height),
        };
        Board {
            width,
            height,
            food,
        }
    }

    pub fn wrap(&self, x: i32, y: i32) -> Position {
        let w = self.width as i32;
        let h = self.height as i32;
        Position {
            x: x.rem_euclid(w) as u16,
            y: y.rem_euclid(h) as u16,
        }
    }

    pub fn spawn_food(&mut self, rng: &mut (impl Rng + ?Sized)) {
        self.food = Position {
            x: rng.random_range(0..self.width),
            y: rng.random_range(0..self.height),
        };
    }

    pub fn food(&self) -> Position {
        self.food
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn board(width: u16, height: u16) -> Board {
        let mut rng = StdRng::seed_from_u64(0);
        Board::new(width, height, &mut rng)
    }

    #[test]
    fn wrap_x_overflow() {
        let b = board(64, 32);
        assert_eq!(b.wrap(64, 0), Position { x: 0, y: 0 });
    }

    #[test]
    fn wrap_x_underflow() {
        let b = board(64, 32);
        assert_eq!(b.wrap(-1, 0), Position { x: 63, y: 0 });
    }

    #[test]
    fn wrap_y_underflow() {
        let b = board(64, 32);
        assert_eq!(b.wrap(0, -1), Position { x: 0, y: 31 });
    }

    #[test]
    fn wrap_y_overflow() {
        let b = board(64, 32);
        assert_eq!(b.wrap(0, 32), Position { x: 0, y: 0 });
    }

    #[test]
    fn food_within_bounds() {
        let mut rng = StdRng::seed_from_u64(42);
        let b = Board::new(64, 32, &mut rng);
        let f = b.food();
        assert!(f.x < 64);
        assert!(f.y < 32);
    }

    #[test]
    fn spawn_food_within_bounds() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut b = Board::new(8, 8, &mut rng);
        for _ in 0..100 {
            b.spawn_food(&mut rng);
            let f = b.food();
            assert!(f.x < 8);
            assert!(f.y < 8);
        }
    }
}
