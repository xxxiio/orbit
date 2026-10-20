FROM rust:1-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p orbit-server

FROM debian:bookworm-slim
RUN useradd -r -u 10001 orbit && mkdir /data && chown orbit:orbit /data
COPY --from=builder /src/target/release/orbit-server /usr/local/bin/orbit-server
USER orbit
ENV ORBIT_SERVER_DB=/data/orbit-server.db ORBIT_SERVER_BIND=0.0.0.0:8787
EXPOSE 8787
VOLUME /data
ENTRYPOINT ["/usr/local/bin/orbit-server"]
