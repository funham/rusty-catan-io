# bot training
cargo run -p catan-train -- \
  --iterations 10000 \
  --bots random,ml_linear,ml_deep \
  --logs ./ai_logs

# online game
cargo run -p catan-server -- \
  --port 8080

# default local game
cargo run --bin catan-runtime

# bot benchmark
cargo run --release --bin catan-bench -- \
  --games 1000 \
  --no-log

# bot benchmark with legal-move counters
cargo run --release --features bench-counters --bin catan-bench -- \
  --games 1000 \
  --no-log
