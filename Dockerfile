# startpage-server — multi-stage build: compile the Rust binary, then run it
# in a slim image with the static web root beside it.
FROM rust:1-bookworm AS build
WORKDIR /build
COPY server/ ./server/
RUN cargo build --release --manifest-path server/Cargo.toml

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --create-home --home-dir /home/startpage startpage \
    && mkdir -p /app/web /data \
    && chown -R startpage:startpage /app /data
COPY --from=build /build/server/target/release/startpage-server /app/startpage-server
COPY index.html /app/web/index.html
COPY src/ /app/web/src/
COPY packaging/ /app/packaging/
# Source files may arrive mode 660 (zip/context preserved); the server runs
# as the unprivileged startpage user and must be able to read the web root.
RUN chmod -R a+rX /app/web /app/packaging
USER startpage
ENV DATA_DIR=/data WEB_DIR=/app/web PACKAGING_DIR=/app/packaging PORT=8080
EXPOSE 8080
VOLUME /data
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
    CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8080 && printf 'GET /healthz HTTP/1.0\\r\\n\\r\\n' >&3 && grep -q ok <&3"]
ENTRYPOINT ["/app/startpage-server"]
