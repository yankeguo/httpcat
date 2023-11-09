FROM ghcr.io/rust-cross/rust-musl-cross:x86_64-musl AS builder
WORKDIR /workspace
COPY . .
RUN cargo build --release

FROM scratch
COPY --from=builder /workspace/target/x86_64-unknown-linux-musl/release/httpcat /httpcat
CMD ["/httpcat"]
