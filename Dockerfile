FROM rust:1.93.1-slim-bullseye AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && curl -L https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64 \
       -o /usr/local/bin/cloudflared \
    && chmod +x /usr/local/bin/cloudflared \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/oxidar-snake /usr/local/bin/oxidar-snake
COPY config.toml /etc/oxidar-snake/config.toml
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 9001 9002

HEALTHCHECK CMD curl -f http://localhost:9002/health || exit 1

ENTRYPOINT ["/entrypoint.sh"]
