set -ex

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT/geomedea-wasm"

wasm-pack build \
    --out-name geomedea \
    --target web \
    --release \
    . \
    --config 'profile.release.opt-level="z"' \
    --config 'profile.release.lto=true' \
    --config 'profile.release.codegen-units=1' \

ls -l pkg/geomedea_bg.wasm

