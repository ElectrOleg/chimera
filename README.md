# Chimera Protocol 🧬
**Evolutionary, Polymorphic, AI-Driven Network Protocol**

Chimera is a next-generation network protocol designed to survive in hostile network environments (DPI, Censorship, Unstable Links). It uses **Polymorphism** to mimic legitimate traffic (HTTP) and **AI Routing** to dynamically switch transports when blocked.

## 🚀 Key Features

*   **🦎 Polymorphic Camouflage**: Handshake looks like legitimate HTTP traffic to DPI.
*   **🧠 AI-Driven Routing**: Automatically detects packet loss/latency and switches paths (TCP <-> FakeTCP <-> QUIC).
*   **🛡️ Post-Quantum Security**: X25519 (Elliptic Curve) + ChaCha20-Poly1305 encryption.
*   **🔄 Reactive Transport Mutation**: If a protocol is blocked (RST/Drop), the client instantly switches to a fallback.

## 📦 Quick Start (Docker)

The fastest and safest way to run Chimera is using Docker. This isolates the protocol from your host system.

### Prerequisites
- Docker & Docker Compose installed.

### Run Server & Client

```bash
# Start the entire stack (Server + Client) in background
docker-compose up -d --build

# View logs
docker-compose logs -f
```

### Stop & Clean Up

```bash
# Stop containers
docker-compose down
```

## 🌍 Real-World Deployment (Linux Server + Mac Client)

### 1. Server (Linux/Ubuntu/VPS)
Clone the repo and start **only the server**:
```bash
# git clone ... && cd chimera_protocol
docker-compose up -d --build server
```
This starts the Chimera Server on port `8080`. Ensure your firewall allows inbound traffic on TCP/8080.

### 2. Client (Your Mac)
You can run the client natively or in Docker, pointing it to your server's IP.

**Option A: Native Rust (Recommended for dev)**
```bash
# Replace x.x.x.x with your Linux server's public IP
export SERVER_HOST=x.x.x.x
cargo run -p chimera_core --bin client
```

**Option B: Docker**
```bash
# Run a one-off client container
docker-compose run -e SERVER_HOST=x.x.x.x --rm client
```

## 🛠 Manual Installation (Rust)

If you want to develop or run natively:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Run Server
cargo run -p chimera_core --bin server

# Run Client (in another terminal)
cargo run -p chimera_core --bin client
```

## 🧪 Architecture

*   **`chimera_core`**: Main engine (Server listener, Connection handling).
*   **`chimera_transport`**: Pluggable transport layer (TCP, BlockedProtocol, etc.).
*   **`chimera_crypto`**: Cryptographic primitives (`ring` based).
*   **`chimera_ai`**: Heuristic engine for path selection and penalty logic.

## ⚠️ Disclaimer
This is a research prototype. Do not use for critical security needs without further auditing.
