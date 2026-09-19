# Agentrix OS — production image
#
# Build from the parent directory so path deps resolve:
#   cd .. && docker build -f spatial-os/Dockerfile -t agentrix-os .
#
# Expects sibling directories: adk-rust/, spatial-os/
# Optional: mount MCP server binaries at runtime (see deploy/docker-compose.yml).

# adk-rust declares rust-version = "1.95" (rmcp 3, time 0.3.51 need ≥ 1.88); keep this in step
# with the toolchain the team builds with.
FROM rust:1.95-bookworm AS builder

WORKDIR /build
COPY adk-rust /build/adk-rust
COPY spatial-os /build/spatial-os

WORKDIR /build/spatial-os
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/spatial-os/target/release/spatial-os /usr/local/bin/spatial-os
COPY --from=builder /build/spatial-os/web /app/web
COPY --from=builder /build/spatial-os/audio /app/audio
COPY --from=builder /build/spatial-os/migrations /app/migrations
COPY --from=builder /build/spatial-os/business.toml /app/business.toml
COPY --from=builder /build/spatial-os/mcp_allowlists.toml /app/mcp_allowlists.toml

ENV HOST=0.0.0.0 \
    PORT=9847 \
    RUST_LOG=info \
    ARTIFACT_DIR=/app/artifacts

RUN mkdir -p /app/artifacts

EXPOSE 9847

CMD ["spatial-os"]