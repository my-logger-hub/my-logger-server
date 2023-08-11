FROM rust:slim
COPY ./target/release/my-logger-server ./target/release/my-logger-server 
ENTRYPOINT ["./target/release/my-logger-server"]