# The release workflow builds the API binary, which embeds the dashboard, into
# dist/api. The base matches the runner so the binary finds the same glibc.
FROM ubuntu:22.04
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --chmod=755 dist/api/daroyan-api /usr/local/bin/daroyan-api
ENV HOST=::
EXPOSE 8900
CMD ["daroyan-api"]
