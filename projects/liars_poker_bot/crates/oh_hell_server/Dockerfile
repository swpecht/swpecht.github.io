FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY oh-hell-server-bin /usr/local/bin/oh-hell-server
# main.rs writes oh_hell_server.log to the cwd. /tmp is world-writable so the
# container UID (heron) can open it regardless of how COPY's defaults left
# ownership. stdout logs still flow to Loki via the alloy docker scrape.
WORKDIR /tmp
EXPOSE 4001
CMD ["oh-hell-server"]
