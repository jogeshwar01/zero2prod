FROM rust:1.87-slim

WORKDIR /app

COPY . .

ENV SQLX_OFFLINE=true

RUN cargo build --release

ENV APP_ENVIRONMENT=prod

ENTRYPOINT ["./target/release/zero2prod"]