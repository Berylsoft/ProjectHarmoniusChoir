FROM rust:1.88 AS chef

WORKDIR /app
RUN cargo install cargo-chef

FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS run

WORKDIR /app
COPY --from=build /app/target/release/backend /usr/local/bin
RUN mkdir data

ENTRYPOINT ["/usr/local/bin/backend"]
