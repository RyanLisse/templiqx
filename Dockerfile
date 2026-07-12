# rust:1.88-alpine multi-platform index digest from Docker Hub. Alpine's
# toolchain targets musl by default, producing static binaries with no libc
# CVE surface, so the runtime stage can use distroless "static" (no glibc,
# no package manager, nothing left for a vulnerability scanner to flag).
FROM rust:1.88-alpine@sha256:9dfaae478ecd298b6b5a039e1f2cc4fc040fc818a2de9aa78fa714dea036574d AS builder

RUN apk add --no-cache musl-dev=1.2.5-r12
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY adapters ./adapters
COPY tools ./tools
COPY examples ./examples
RUN cargo build --release -p templiqx-cli -p templiqx-mcp -p templiqx-mock-gateway -p templiqx-http-conformance

FROM gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478 AS templiqx-cli
ARG TARGETPLATFORM
LABEL org.opencontainers.image.title="templiqx"
LABEL org.opencontainers.image.description="Synthetic pre-CRM3 conformance CLI"
LABEL org.opencontainers.image.source="https://github.com/local/templiqx"
LABEL org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478"
LABEL org.opencontainers.image.platform=$TARGETPLATFORM
COPY --from=builder /src/target/release/templiqx /usr/local/bin/templiqx
COPY --from=builder /src/target/release/templiqx-mcp /usr/local/bin/templiqx-mcp
COPY --from=builder /src/target/release/templiqx-mock-gateway /usr/local/bin/templiqx-mock-gateway
COPY --from=builder /src/target/release/templiqx-http-conformance /usr/local/bin/templiqx-http-conformance
COPY --from=builder /src/examples /packages
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/templiqx"]

FROM gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478 AS templiqx-mcp
ARG TARGETPLATFORM
LABEL org.opencontainers.image.title="templiqx-mcp"
LABEL org.opencontainers.image.description="Slim MCP stdio transport for Templiqx"
LABEL org.opencontainers.image.source="https://github.com/local/templiqx"
LABEL org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot@sha256:d29e660cc75a5b6b1334e03c5c81ccf9bc0884a002c6000dbf0fb96034814478"
LABEL org.opencontainers.image.platform=$TARGETPLATFORM
COPY --from=builder /src/target/release/templiqx-mcp /usr/local/bin/templiqx-mcp
COPY --from=builder /src/examples /packages
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/templiqx-mcp"]
CMD ["/packages"]
