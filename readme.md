# bot training
cargo run -p catan-train -- \
  --iterations 10000 \
  --bots random,ml_linear,ml_deep \
  --logs ./ai_logs

# online game
cargo run -p catan-server -- \
  --port 8080
