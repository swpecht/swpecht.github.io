FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libstdc++6 && rm -rf /var/lib/apt/lists/*
# CPU libtorch runtime: the binary links tch dynamically via the
# card_platypus dependency (even though the Catan bot itself is pure
# PIMCTS). The deploy stages the needed .so files from the build host's
# libtorch-cpu tree into the build context, same as oh_hell_server.
COPY libtorch-lib/ /opt/libtorch/lib/
ENV LD_LIBRARY_PATH=/opt/libtorch/lib
COPY catan-server-bin /usr/local/bin/catan-server
# main.rs writes catan_server.log to the cwd. /tmp is world-writable so the
# container UID can open it regardless of how COPY's defaults left
# ownership. stdout logs still flow to Loki via the alloy docker scrape.
WORKDIR /tmp
EXPOSE 4002
CMD ["catan-server"]
