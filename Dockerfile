# rust:1.88-alpine multi-platform index digest from Docker Hub. Alpine's
# toolchain targets musl by default, producing static binaries with no libc
# CVE surface, so the runtime stage can use distroless "static" (no glibc,
# no package manager, nothing left for a vulnerability scanner to flag).
FROM rust:1.88-alpine@sha256:9dfaae478ecd298b6b5a039e1f2cc4fc040fc818a2de9aa78fa714dea036574d AS source

RUN apk add --no-cache musl-dev=1.2.5-r12
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY adapters ./adapters
COPY tools ./tools
COPY examples ./examples
COPY openapi ./openapi

# Product and conformance binaries are built in separate stages so a product
# image can never acquire mock tooling through an over-broad COPY.
FROM source AS product-builder
RUN cargo build --release -p templiqx-cli -p templiqx-mcp

FROM source AS http-server-builder
RUN cargo build --release -p templiqx-http-server \
    && mkdir -p /runtime-data /runtime-workspace

FROM source AS conformance-builder
RUN cargo build --release -p templiqx-mock-gateway -p templiqx-http-conformance

FROM gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478 AS templiqx-cli
ARG TARGETPLATFORM
ARG VERSION=0.1.0
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="templiqx-cli"
LABEL org.opencontainers.image.description="Provider-neutral Templiqx contract compiler CLI"
LABEL org.opencontainers.image.source="https://github.com/RyanLisse/templiqx"
LABEL org.opencontainers.image.url="https://github.com/RyanLisse/templiqx"
LABEL org.opencontainers.image.version=$VERSION
LABEL org.opencontainers.image.revision=$VCS_REF
LABEL org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478"
LABEL org.opencontainers.image.platform=$TARGETPLATFORM
COPY --from=product-builder /src/target/release/templiqx /usr/local/bin/templiqx
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/templiqx"]

FROM gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478 AS templiqx-mcp
ARG TARGETPLATFORM
ARG VERSION=0.1.0
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="templiqx-mcp"
LABEL org.opencontainers.image.description="Slim MCP stdio transport for Templiqx"
LABEL org.opencontainers.image.source="https://github.com/RyanLisse/templiqx"
LABEL org.opencontainers.image.url="https://github.com/RyanLisse/templiqx"
LABEL org.opencontainers.image.version=$VERSION
LABEL org.opencontainers.image.revision=$VCS_REF
LABEL org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478"
LABEL org.opencontainers.image.platform=$TARGETPLATFORM
COPY --from=product-builder /src/target/release/templiqx-mcp /usr/local/bin/templiqx-mcp
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/templiqx-mcp"]

FROM gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478 AS templiqx-http-server
ARG TARGETPLATFORM
ARG VERSION=0.1.0
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="templiqx-http-server"
LABEL org.opencontainers.image.description="Local/demo Operations HTTP server (deterministic-fake by default). Not an official signed release artifact."
LABEL io.templiqx.artifact.class="local-demo-not-signed-release"
LABEL io.templiqx.runtime.default-mode="deterministic-fake"
LABEL org.opencontainers.image.source="https://github.com/RyanLisse/templiqx"
LABEL org.opencontainers.image.url="https://github.com/RyanLisse/templiqx"
LABEL org.opencontainers.image.version=$VERSION
LABEL org.opencontainers.image.revision=$VCS_REF
LABEL org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478"
LABEL org.opencontainers.image.platform=$TARGETPLATFORM
COPY --from=http-server-builder /src/target/release/templiqx-http-server /usr/local/bin/templiqx-http-server
COPY --from=http-server-builder --chown=65532:65532 /runtime-data /data
COPY --from=http-server-builder --chown=65532:65532 /runtime-workspace /workspace
USER 65532:65532
WORKDIR /data
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/templiqx-http-server"]

FROM gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478 AS templiqx-conformance
ARG TARGETPLATFORM
ARG VERSION=0.1.0
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="templiqx-conformance"
LABEL org.opencontainers.image.description="Synthetic Templiqx mock gateway and HTTP conformance tooling"
LABEL org.opencontainers.image.source="https://github.com/RyanLisse/templiqx"
LABEL org.opencontainers.image.url="https://github.com/RyanLisse/templiqx"
LABEL org.opencontainers.image.version=$VERSION
LABEL org.opencontainers.image.revision=$VCS_REF
LABEL org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478"
LABEL org.opencontainers.image.platform=$TARGETPLATFORM
LABEL io.templiqx.artifact.class="synthetic-conformance-only"
COPY --from=conformance-builder /src/target/release/templiqx-mock-gateway /usr/local/bin/templiqx-mock-gateway
COPY --from=conformance-builder /src/target/release/templiqx-http-conformance /usr/local/bin/templiqx-http-conformance
COPY --from=conformance-builder /src/examples /packages
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/templiqx-http-conformance"]
