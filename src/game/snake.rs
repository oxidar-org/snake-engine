use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::board::{Board, Position};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum Direction {
    Up = 0,
    Right = 1,
    Down = 2,
    Left = 3,
}

impl Direction {
    pub fn opposite(&self) -> Direction {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }

    pub fn from_u8(val: u8) -> Option<Direction> {
        match val {
            0 => Some(Direction::Up),
            1 => Some(Direction::Right),
            2 => Some(Direction::Down),
            3 => Some(Direction::Left),
            _ => None,
        }
    }

    pub fn delta(&self) -> (i32, i32) {
        match self {
            Direction::Up => (0, -1),
            Direction::Down => (0, 1),
            Direction::Left => (-1, 0),
            Direction::Right => (1, 0),
        }
    }
}

pub struct Snake {
    pub name: String,
    pub body: VecDeque<Position>,
    pub dir: Direction,
    pub crowns: u32,
    pub next_dir: Option<Direction>,
    pub growing: u32,
}

impl Snake {
    pub fn new(
        name: String,
        start_pos: Position,
        dir: Direction,
        length: u16,
        board: &Board,
    ) -> Snake {
        let mut body = VecDeque::with_capacity(length as usize);
        body.push_back(start_pos);

        let opposite = dir.opposite().delta();
        for _ in 1..length {
            let prev = *body.back().unwrap();
            let pos = board.wrap(prev.x as i32 + opposite.0, prev.y as i32 + opposite.1);
            body.push_back(pos);
        }

        Snake {
            name,
            body,
            dir,
            crowns: 0,
            next_dir: None,
            growing: 0,
        }
    }

    pub fn queue_turn(&mut self, dir: Direction) {
        if dir != self.dir.opposite() {
            self.next_dir = Some(dir);
        }
    }

    pub fn apply_turn(&mut self) {
        if let Some(dir) = self.next_dir.take() {
            self.dir = dir;
        }
    }

    pub fn advance(&mut self, board: &Board) {
        let (dx, dy) = self.dir.delta();
        let head = self.head();
        let new_head = board.wrap(head.x as i32 + dx, head.y as i32 + dy);
        self.body.push_front(new_head);

        if self.growing > 0 {
            self.growing -= 1;
        } else {
            self.body.pop_back();
        }
    }

    pub fn grow(&mut self) {
        self.growing += 1;
    }

    pub fn head(&self) -> Position {
        self.body[0]
    }

    pub fn len(&self) -> usize {
        self.body.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn test_board() -> Board {
        let mut rng = StdRng::seed_from_u64(0);
        Board::new(8, 8, &mut rng)
    }

    #[test]
    fn advance_moves_head_keeps_length() {
        let board = test_board();
        let mut snake = Snake::new(
            "test".into(),
            Position { x: 4, y: 4 },
            Direction::Right,
            4,
            &board,
        );
        let original_len = snake.len();
        snake.advance(&board);
        assert_eq!(snake.head(), Position { x: 5, y: 4 });
        assert_eq!(snake.len(), original_len);
    }

    #[test]
    fn grow_then_advance_increases_length() {
        let board = test_board();
        let mut snake = Snake::new(
            "test".into(),
            Position { x: 4, y: 4 },
            Direction::Right,
            4,
            &board,
        );
        snake.grow();
        snake.advance(&board);
        assert_eq!(snake.len(), 5);
    }

    #[test]
    fn queue_reversal_ignored() {
        let board = test_board();
        let mut snake = Snake::new(
            "test".into(),
            Position { x: 4, y: 4 },
            Direction::Right,
            4,
            &board,
        );
        snake.queue_turn(Direction::Left);
        snake.apply_turn();
        assert_eq!(snake.dir, Direction::Right);
    }

    #[test]
    fn queue_valid_turn_apply_advance() {
        let board = test_board();
        let mut snake = Snake::new(
            "test".into(),
            Position { x: 4, y: 4 },
            Direction::Right,
            4,
            &board,
        );
        snake.queue_turn(Direction::Up);
        snake.apply_turn();
        snake.advance(&board);
        assert_eq!(snake.dir, Direction::Up);
        assert_eq!(snake.head(), Position { x: 4, y: 3 });
    }

    #[test]
    fn movement_wraps_at_edges() {
        let board = test_board();
        let mut snake = Snake::new(
            "test".into(),
            Position { x: 0, y: 0 },
            Direction::Left,
            4,
            &board,
        );
        snake.advance(&board);
        assert_eq!(snake.head(), Position { x: 7, y: 0 });
    }
}
