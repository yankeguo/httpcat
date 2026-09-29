FROM ghcr.io/rust-cross/rust-musl-cross:x86_64-musl AS builder
WORKDIR /workspace
COPY . .
RUN rustup target add x86_64-unknown-linux-musl \
    && cargo build --release --locked \
    && musl-strip target/x86_64-unknown-linux-musl/release/httpcat

FROM scratch
COPY --from=builder /workspace/target/x86_64-unknown-linux-musl/release/httpcat /httpcat
EXPOSE 80
ENTRYPOINT ["/httpcat"]
