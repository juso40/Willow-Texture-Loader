#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

docker build --network host -t texloader-native "$repo_root/.devcontainer"
docker run --rm --network host \
    -v "$repo_root":/work \
    -v texloader-cargo:/usr/local/cargo/registry \
    -v texloader-target:/work/native/target \
    texloader-native
