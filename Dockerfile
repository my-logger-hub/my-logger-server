FROM ubuntu:22.04
COPY ./target/release/my-logger-server ./target/release/my-logger-server 
ENTRYPOINT ["./target/release/my-logger-server"]