# Quorum: multi-stage build producing a static (musl) binary with the web UI embedded.
# Build from the repository root:  docker build -f deploy/Dockerfile -t quorum .

FROM node:22-alpine AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1-alpine AS server
RUN apk add --no-cache build-base
WORKDIR /src
COPY backend/ backend/
COPY --from=web /src/web/build web/build
WORKDIR /src/backend
RUN cargo build --release --locked && strip target/release/quorum-server

FROM alpine:3
RUN apk add --no-cache ca-certificates tzdata \
    && adduser -D -H -u 10001 quorum \
    && mkdir -p /var/lib/quorum /etc/quorum \
    && chown quorum:quorum /var/lib/quorum
COPY --from=server /src/backend/target/release/quorum-server /usr/local/bin/quorum-server
USER quorum
ENV QUORUM_BIND=0.0.0.0:8080 \
    QUORUM_DATABASE=/var/lib/quorum/quorum.db
VOLUME ["/var/lib/quorum"]
EXPOSE 8080
# QUORUM_SECRET_KEY must be provided at runtime (e.g. --env-file).
ENTRYPOINT ["/usr/local/bin/quorum-server"]
CMD ["serve"]
