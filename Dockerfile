FROM rust:1.86.0-slim-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY index.html ./index.html
COPY runtime-assets ./runtime-assets
COPY web ./web
RUN cargo build --release -p marknest -p marknest-server

FROM node:22-bookworm-slim AS playwright-runtime

WORKDIR /app/crates/marknest/playwright-runtime
COPY crates/marknest/playwright-runtime/package.json ./
COPY crates/marknest/playwright-runtime/package-lock.json ./
RUN npm ci --omit=dev

FROM debian:bookworm-slim

ENV MARKNEST_BROWSER_PATH=/usr/bin/chromium
ENV MARKNEST_PLAYWRIGHT_RUNTIME_DIR=/opt/marknest/playwright-runtime
ENV MARKNEST_SERVER_ADDR=0.0.0.0:3476

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        chromium \
        fonts-dejavu-core \
        fonts-noto-cjk \
        nodejs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/marknest /usr/local/bin/marknest
COPY --from=builder /app/target/release/marknest-server /usr/local/bin/marknest-server
COPY crates/marknest/playwright-runtime /opt/marknest/playwright-runtime
COPY --from=playwright-runtime /app/crates/marknest/playwright-runtime/node_modules /opt/marknest/playwright-runtime/node_modules

EXPOSE 3476
CMD ["marknest-server"]
