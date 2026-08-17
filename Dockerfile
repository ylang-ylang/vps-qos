FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM alpine:3.21
RUN apk add --no-cache fping iproute2
WORKDIR /app
COPY --from=builder /src/target/release/vps-bandwidth-observer /app/vps-bandwidth-observer
COPY config/default.json /app/config/default.json
ENTRYPOINT ["/app/vps-bandwidth-observer"]
CMD ["/app/config/default.json"]
