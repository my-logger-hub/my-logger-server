FROM ubuntu:22.04
RUN apt-get update && apt-get install -y ca-certificates
COPY ./target/release/my-logger-server ./target/release/my-logger-server 
ENTRYPOINT ["./target/release/my-logger-server"]