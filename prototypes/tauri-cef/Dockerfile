FROM rust:1.99.1-trixie
RUN apt-get update && apt-get install -y --no-install-recommends \
    libgtk-4-dev libx11-dev libxdo-dev libnss3 libasound2t64 libgbm1 \
    libatk-bridge2.0-0 libdrm2 xvfb xauth dbus-x11 ca-certificates black \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy
WORKDIR /probe
ENV CARGO_HOME=/cache/cargo
