# rust:1.88-bookworm multi-platform index digest from Docker Hub.
FROM rust:1.88-bookworm@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY adapters ./adapters
COPY tools ./tools
COPY examples ./examples
RUN cargo build --release -p templiqx-cli -p templiqx-mcp -p templiqx-mock-gateway -p templiqx-http-conformance

FROM debian:bookworm-slim@sha256:60eac759739651111db372c07be67863818726f754804b8707c90979bda511df AS templiqx-cli
ARG TARGETPLATFORM
LABEL org.opencontainers.image.title="templiqx"
LABEL org.opencontainers.image.description="Synthetic pre-CRM3 conformance CLI"
LABEL org.opencontainers.image.source="https://github.com/local/templiqx"
LABEL org.opencontainers.image.base.name="debian:bookworm-slim@sha256:60eac759739651111db372c07be67863818726f754804b8707c90979bda511df"
LABEL org.opencontainers.image.platform=$TARGETPLATFORM
COPY --from=builder /src/target/release/templiqx /usr/local/bin/templiqx
COPY --from=builder /src/target/release/templiqx-mcp /usr/local/bin/templiqx-mcp
COPY --from=builder /src/target/release/templiqx-mock-gateway /usr/local/bin/templiqx-mock-gateway
COPY --from=builder /src/target/release/templiqx-http-conformance /usr/local/bin/templiqx-http-conformance
COPY --from=builder /src/examples /packages
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/templiqx"]
