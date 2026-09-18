# syntax=docker/dockerfile:1
# check=error=true
FROM rust:1.98.1-alpine3.24 AS build

WORKDIR /build

COPY --link Cargo.toml Cargo.lock ./
COPY --link src ./src

RUN <<'EOF'
  cargo build --locked --release
EOF


FROM alpine:3.24

EXPOSE 80

ENV IMDS_LISTEN_PORT=8181

RUN <<'EOF'
  apk add --no-cache \
    ca-certificates \
    curl \
    iproute2 \
    iptables
EOF

COPY --link --from=build /build/target/release/ec2-metadata-mock /usr/local/bin/ec2-metadata-mock
COPY --link --chmod=755 docker-entrypoint.sh /docker-entrypoint.sh

HEALTHCHECK \
  --interval=5s \
  --timeout=2s \
  --retries=12 \
  CMD [ \
    "/bin/sh", "-c", \
    "curl --fail --silent http://169.254.169.254/health && \
      if [ \"${IMDS_IPV6_ENABLED:-0}\" = 1 ]; then \
        curl --fail --silent 'http://[fd00:ec2::254]/health'; \
      fi" \
  ]

ENTRYPOINT ["/docker-entrypoint.sh"]
