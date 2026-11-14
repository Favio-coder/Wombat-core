FROM rust:1.98-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev clang && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release --bin wombat-node

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/wombat-node /usr/local/bin/wombat-node
COPY contracts /app/contracts
CMD ["wombat-node", "run", "--demo"]
