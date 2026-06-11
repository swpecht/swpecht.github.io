FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libstdc++6 && rm -rf /var/lib/apt/lists/*
# CPU libtorch runtime for the R-NaD transformer agent. The binary links
# tch dynamically; the deploy stages the three needed .so files from the
# build host's libtorch-cpu tree (2.5.0+cpu) into the build context.
COPY libtorch-lib/ /opt/libtorch/lib/
ENV LD_LIBRARY_PATH=/opt/libtorch/lib
COPY oh-hell-server-bin /usr/local/bin/oh-hell-server
# main.rs writes oh_hell_server.log to the cwd. /tmp is world-writable so the
# container UID (heron) can open it regardless of how COPY's defaults left
# ownership. stdout logs still flow to Loki via the alloy docker scrape.
WORKDIR /tmp
EXPOSE 4001
CMD ["oh-hell-server"]
