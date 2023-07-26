#!/bin/bash

mkdir -p ~/.config/zellij/plugins/

cargo build --release
mv target/wasm32-wasi/release/harpoon.wasm ~/.config/zellij/plugins/

git clone git@github.com:nacho114/cached.git
cd cached
cargo build --release
mv target/wasm32-wasi/release/cached.wasm ~/.config/zellij/plugins/
