#!/bin/sh
set -e

cloudflared tunnel run --token "$CLOUDFLARE_TUNNEL_TOKEN" &

exec oxidar-snake /etc/oxidar-snake/config.toml
