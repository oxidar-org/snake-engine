use anyhow::Result;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub game: GameConfig,
    pub server: ServerConfig,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct GameConfig {
    pub board_width: u16,
    pub board_height: u16,
    pub max_players: u32,
    pub tick_ms: u64,
    pub snake_start_length: u16,
    pub snake_win_length: u16,
    pub disconnect_timeout_s: u64,
    pub leaderboard_interval_ticks: u64,
    #[serde(default = "default_chaos_interval_ticks")]
    pub chaos_interval_ticks: u64,
    #[serde(default)]
    pub palette: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_health_port")]
    pub health_port: u16,
}

fn default_health_port() -> u16 {
    9002
}

fn default_chaos_interval_ticks() -> u64 {
    10
}

fn hsl_to_hex(h: f64, s: f64, l: f64) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let r = ((r1 + m) * 255.0).round() as u8;
    let g = ((g1 + m) * 255.0).round() as u8;
    let b = ((b1 + m) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

pub fn generate_palette(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| {
            let hue = i as f64 * 360.0 / n as f64;
            hsl_to_hex(hue, 0.75, 0.55)
        })
        .collect()
}

impl Config {
    pub fn load(path: &str) -> Result<Config> {
        let content = fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&content)?;
        if config.game.palette.is_empty() {
            config.game.palette = generate_palette(config.game.max_players as usize);
        }
        Ok(config)
    }

    #[cfg(test)]
    pub fn load_from_str(content: &str) -> Result<Config> {
        let mut config: Config = toml::from_str(content)?;
        if config.game.palette.is_empty() {
            config.game.palette = generate_palette(config.game.max_players as usize);
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn from_str(s: &str) -> Result<Config, toml::de::Error> {
        toml::from_str(s)
    }

    const VALID_TOML: &str = r##"
[game]
board_width = 64
board_height = 32
max_players = 32
tick_ms = 200
snake_start_length = 4
snake_win_length = 16
disconnect_timeout_s = 60
leaderboard_interval_ticks = 25
chaos_interval_ticks = 5
palette = ["#FF0000", "#00FF00"]

[server]
host = "0.0.0.0"
port = 9001
health_port = 9002
"##;

    #[test]
    fn load_valid_config() {
        let config = from_str(VALID_TOML).unwrap();

        assert_eq!(config.game.board_width, 64);
        assert_eq!(config.game.board_height, 32);
        assert_eq!(config.game.max_players, 32);
        assert_eq!(config.game.tick_ms, 200);
        assert_eq!(config.game.snake_start_length, 4);
        assert_eq!(config.game.snake_win_length, 16);
        assert_eq!(config.game.disconnect_timeout_s, 60);
        assert_eq!(config.game.leaderboard_interval_ticks, 25);
        assert_eq!(config.game.chaos_interval_ticks, 5);
        assert_eq!(config.game.palette, vec!["#FF0000", "#00FF00"]);
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9001);
        assert_eq!(config.server.health_port, 9002);
    }

    #[test]
    fn missing_palette_deserializes_to_empty() {
        let toml = r##"
[game]
board_width = 64
board_height = 32
max_players = 32
tick_ms = 200
snake_start_length = 4
snake_win_length = 16
disconnect_timeout_s = 60
leaderboard_interval_ticks = 25

[server]
host = "0.0.0.0"
port = 9001
"##;
        let config = from_str(toml).unwrap();
        assert!(config.game.palette.is_empty());
    }

    #[test]
    fn empty_palette_auto_generates_on_load() {
        let toml = r##"
[game]
board_width = 64
board_height = 32
max_players = 32
tick_ms = 200
snake_start_length = 4
snake_win_length = 16
disconnect_timeout_s = 60
leaderboard_interval_ticks = 25
palette = []

[server]
host = "0.0.0.0"
port = 9001
"##;
        let config: Config = from_str(toml).unwrap();
        // Deserialization succeeds and palette is empty before load() auto-generates
        assert!(config.game.palette.is_empty());
    }

    #[test]
    fn chaos_interval_ticks_defaults_to_ten() {
        let toml = r##"
[game]
board_width = 64
board_height = 32
max_players = 32
tick_ms = 200
snake_start_length = 4
snake_win_length = 16
disconnect_timeout_s = 60
leaderboard_interval_ticks = 25

[server]
host = "0.0.0.0"
port = 9001
"##;
        let config = Config::load_from_str(toml).unwrap();
        assert_eq!(config.game.chaos_interval_ticks, 10);
    }

    #[test]
    fn generate_palette_produces_unique_colors() {
        let palette = generate_palette(128);
        let unique: HashSet<&String> = palette.iter().collect();
        assert_eq!(unique.len(), 128);
    }

    #[test]
    fn generate_palette_produces_valid_hex() {
        let palette = generate_palette(128);
        for color in &palette {
            assert_eq!(color.len(), 7, "color must be 7 chars: {color}");
            assert!(color.starts_with('#'), "color must start with #: {color}");
            let hex_chars = &color[1..];
            assert!(
                hex_chars.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()),
                "color must be uppercase hex: {color}"
            );
        }
    }
}
