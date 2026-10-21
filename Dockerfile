# The rustable CLI, from the static musl binaries CI already built: nothing is compiled here.
# CI's `build` jobs leave them at dist/docker/<arch>/rustable; `docker-image` runs
#   docker buildx build --platform linux/amd64,linux/arm64 .
#
#   docker run --rm -v "$PWD:/work" <image> analyze
#   docker run --rm -v "$PWD:/work" -e RUSTABLE_DB_HOST=… <image> db migrate
FROM scratch
ARG TARGETARCH
COPY dist/docker/${TARGETARCH}/rustable /usr/local/bin/rustable
WORKDIR /work
ENV RUSTABLE_CONFIG_PATH=/work/config.toml
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/rustable"]
CMD ["--help"]
