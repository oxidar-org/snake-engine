FROM rust:1.93.1-slim-bullseye AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/oxidar-snake /usr/local/bin/oxidar-snake
COPY config.toml /etc/oxidar-snake/config.toml

EXPOSE 9001 9002

HEALTHCHECK CMD curl -f http://localhost:9002/health || exit 1

ENTRYPOINT ["oxidar-snake", "/etc/oxidar-snake/config.toml"]
