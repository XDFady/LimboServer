#!/bin/bash
set -e

mkdir -p instances logs

BIN="./target/release/pico_limbo"

# CHANGE THIS PATH if your server.toml is somewhere else
BASE_CONFIG="./server.toml"

MIRROR_SERVER="mc.hypixel.net:25565"

if [ ! -f "$BIN" ]; then
  echo "ERROR: Binary not found: $BIN"
  echo "Run: cargo build --release -p pico_limbo --bin pico_limbo"
  exit 1
fi

if [ ! -f "$BASE_CONFIG" ]; then
  echo "ERROR: Config file not found: $BASE_CONFIG"
  echo "Find it using:"
  echo "find . -name \"server.toml\""
  exit 1
fi

pkill pico_limbo 2>/dev/null || true

for port in $(seq 25565 25614); do
  cfg="instances/server_$port.toml"

  cp "$BASE_CONFIG" "$cfg"

  if grep -q '^bind = ' "$cfg"; then
    sed -i "s/^bind = .*/bind = \"127.0.0.1:$port\"/" "$cfg"
  else
    echo "bind = \"127.0.0.1:$port\"" >> "$cfg"
  fi

  "$BIN" \
    --config "$cfg" \
    --captcha \
    --captcha-timeout-seconds 60 \
    --captcha-max-attempts 3 \
    --mirror-server "$MIRROR_SERVER" \
    --mirror-refresh-seconds 10 \
    --mirror-timeout-seconds 5 \
    --success-kick-message "Verification successful. Please reconnect to the main server." \
    --failed-kick-message "Captcha failed. Please try again later." \
    --captcha-timeout-kick-message "Captcha timed out. Please try again." \
    > "logs/limbo_$port.log" 2>&1 &

  echo "Started PicoLimbo on port $port with PID $!"
done
