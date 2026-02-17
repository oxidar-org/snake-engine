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

impl Config {
    pub fn load(path: &str) -> Result<Config> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_str(s: &str) -> Result<Config, toml::de::Error> {
        toml::from_str(s)
    }

    const VALID_TOML: &str = r#"
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
health_port = 9002
"#;

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
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9001);
        assert_eq!(config.server.health_port, 9002);
    }
}
