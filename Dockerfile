FROM rust:1.88-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libudev-dev libmpv-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets
RUN cargo build --locked --release
FROM scratch AS export
COPY --from=build /source/target/release/aster /aster
