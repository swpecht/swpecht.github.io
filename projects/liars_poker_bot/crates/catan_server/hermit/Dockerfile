# For hermit: scripts/hermit-deploy.sh catan_server beaver builds the server on
# the deploy host and ships only the stripped binary. The three libtorch
# 2.5.0 .so files it links come from pytorch.org here (hermit's layer cache
# keeps them, so only the first build downloads the ~200 MB archive).
FROM ubuntu:26.04 AS libtorch
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl unzip \
	&& rm -rf /var/lib/apt/lists/*
RUN curl -fL --retry 3 -o /tmp/lt.zip \
	"https://download.pytorch.org/libtorch/cpu/libtorch-cxx11-abi-shared-with-deps-2.5.0%2Bcpu.zip" \
	&& mkdir -p /opt/libtorch/lib \
	&& unzip -j -d /opt/libtorch/lib /tmp/lt.zip \
		libtorch/lib/libtorch_cpu.so libtorch/lib/libc10.so libtorch/lib/libgomp-98b21ff3.so.1 \
	&& rm /tmp/lt.zip

# ubuntu:26.04 because the build host is Ubuntu 26.04: the binary links its
# glibc (2.43), which no Debian image carries yet.
FROM ubuntu:26.04
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libstdc++6 tzdata \
	&& rm -rf /var/lib/apt/lists/*
COPY --from=libtorch /opt/libtorch/lib/ /opt/libtorch/lib/
ENV LD_LIBRARY_PATH=/opt/libtorch/lib
COPY server /usr/local/bin/catan_server
WORKDIR /tmp
EXPOSE 4002
CMD ["catan_server"]
