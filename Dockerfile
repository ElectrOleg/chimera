# Build Stage
FROM rust:1.80-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /app

# Copy workspace configuration
COPY Cargo.toml Cargo.lock ./

# Create dummy crates to cache dependencies
RUN mkdir -p chimera_core/src chimera_transport/src chimera_crypto/src chimera_ai/src
RUN echo "fn main() {}" > chimera_core/src/lib.rs
RUN echo "fn main() {}" > chimera_transport/src/lib.rs
RUN echo "fn main() {}" > chimera_crypto/src/lib.rs
RUN echo "fn main() {}" > chimera_ai/src/lib.rs

# Copy Crate Cargo.tomls
COPY chimera_core/Cargo.toml chimera_core/
COPY chimera_transport/Cargo.toml chimera_transport/
COPY chimera_crypto/Cargo.toml chimera_crypto/
COPY chimera_ai/Cargo.toml chimera_ai/

# Build dependencies only (cached if Cargo.toml/lock unchanged)
RUN cargo build --release

# Copy actual source code
COPY chimera_core/src chimera_core/src
COPY chimera_transport/src chimera_transport/src
COPY chimera_crypto/src chimera_crypto/src
COPY chimera_ai/src chimera_ai/src

# Touch main files to force rebuild of source
RUN touch chimera_core/src/lib.rs chimera_transport/src/lib.rs chimera_crypto/src/lib.rs chimera_ai/src/lib.rs

# Build the release binaries
RUN cargo build --release

# ------------------------------------------------------------------------------
# Final Stage (Server)
# ------------------------------------------------------------------------------
FROM alpine:3.18 AS server
WORKDIR /app
# Install minimal runtime dependencies if needed
RUN apk add --no-cache libgcc
COPY --from=builder /app/target/release/server .
CMD ["./server"]

# ------------------------------------------------------------------------------
# Final Stage (Client)
# ------------------------------------------------------------------------------
FROM alpine:3.18 AS client
WORKDIR /app
RUN apk add --no-cache libgcc
COPY --from=builder /app/target/release/client .
CMD ["./client"]
