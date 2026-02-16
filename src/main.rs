use oxidar_snake::config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let config = Config::load(&path).unwrap_or_else(|e| {
        eprintln!("Failed to load config from {path}: {e}");
        std::process::exit(1);
    });

    tracing::info!(
        board = %format!("{}x{}", config.game.board_width, config.game.board_height),
        max_players = config.game.max_players,
        tick_ms = config.game.tick_ms,
        host = %config.server.host,
        port = config.server.port,
        "Configuration loaded"
    );

    oxidar_snake::net::server::run(config).await
}
