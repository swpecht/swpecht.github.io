FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY euchre-server-bin /usr/local/bin/euchre-server
# main.rs writes euchre_server.log to the cwd. /tmp is world-writable so the
# container UID (platypus) can open it regardless of how COPY's defaults
# left ownership. stdout logs still flow to Loki via the alloy docker scrape.
WORKDIR /tmp
EXPOSE 4000
CMD ["euchre-server"]
