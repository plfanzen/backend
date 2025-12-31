FROM rust:1.92.0-slim-trixie AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /plfanzen

COPY . .

RUN cargo build --release

FROM debian:trixie-slim AS manager

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /plfanzen/target/release/plfanzen-manager /usr/local/bin/plfanzen-manager

EXPOSE 50001

CMD ["plfanzen-manager"]

FROM debian:trixie-slim AS api

COPY --from=builder /plfanzen/target/release/plfanzen-api /usr/local/bin/plfanzen-api

EXPOSE 3000

CMD ["plfanzen-api"]
