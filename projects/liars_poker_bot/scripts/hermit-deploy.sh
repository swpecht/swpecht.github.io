#!/bin/sh
# hermit-deploy.sh <server crate> <hermit app>: build a game server here and
# ship it to hermit (https://hermit.fewworddotrick.com/help with a deploy
# token explains hermit), which packages crates/<crate>/hermit/Dockerfile and
# serves https://<app>.fewappdotrick.com.
#
#   HERMIT_TOKEN=gsvc1.... scripts/hermit-deploy.sh oh_hell_server heron
#   HERMIT_TOKEN=gsvc1.... scripts/hermit-deploy.sh catan_server beaver
#   HERMIT_DRY_RUN=1 scripts/hermit-deploy.sh oh_hell_server heron   # build + stage only
#
# Builds the server here (the workspace is big and links libtorch via tch)
# and ships only the stripped binary; the Dockerfile fetches the three
# libtorch .so files itself (hermit caches that layer). Ships what is
# COMMITTED under this project.
set -eu
crate=$1 app=$2
cd "$(dirname "$0")/.."
hdir=crates/$crate/hermit
[ -f "$hdir/Dockerfile" ] || { echo "no $hdir/Dockerfile" >&2; exit 1; }
if [ -z "${HERMIT_DRY_RUN:-}" ]; then
	: "${HERMIT_TOKEN:?set HERMIT_TOKEN to a gander deploy token (see /help)}"
	if ! git diff --quiet HEAD -- .; then
		echo "uncommitted changes in liars_poker_bot: commit first" >&2
		exit 1
	fi
fi

# tch 0.18 wants libtorch 2.5.0. Fetched once, kept outside the repo.
LIBTORCH=${LIBTORCH:-$HOME/.local/lib/libtorch-cpu}
if [ ! -f "$LIBTORCH/lib/libtorch_cpu.so" ]; then
	echo "fetching libtorch 2.5.0 (CPU) into $LIBTORCH"
	tmp=$(mktemp -d)
	curl -fL --retry 3 -o "$tmp/libtorch.zip" \
		"https://download.pytorch.org/libtorch/cpu/libtorch-cxx11-abi-shared-with-deps-2.5.0%2Bcpu.zip"
	mkdir -p "$(dirname "$LIBTORCH")"
	(cd "$tmp" && unzip -q libtorch.zip && rm -rf "$LIBTORCH" && mv libtorch "$LIBTORCH")
	rm -rf "$tmp"
fi

echo "building $crate"
LIBTORCH=$LIBTORCH LIBTORCH_BYPASS_VERSION_CHECK=1 cargo build --release --bin "$crate"

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp "$hdir/Dockerfile" "$hdir/hermit.yaml" "$stage/"
# The release profile keeps debuginfo (~60 MB of the binary); hermit's upload
# limit is 100 MB and nothing reads it there.
strip -o "$stage/server" "target/release/$crate"

tar czf "$stage.tgz" -C "$stage" .
trap 'rm -rf "$stage" "$stage.tgz"' EXIT
echo "bundle: $(du -h "$stage.tgz" | cut -f1) (hermit's limit is 100 MB)"
if [ -n "${HERMIT_DRY_RUN:-}" ]; then
	(cd "$stage" && ls -la)
	exit 0
fi
echo "deploying"
cat "$stage.tgz" |
	curl -sSN --fail-with-body -X POST -H "Authorization: Bearer $HERMIT_TOKEN" \
		--data-binary @- "https://hermit.fewworddotrick.com/apps/$app/deploy"
