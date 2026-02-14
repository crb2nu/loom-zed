.PHONY: build test lint format format-check version-check check ci wasm wasm-debug clean

build:
	cargo build

test:
	cargo test

lint:
	cargo clippy -- -D warnings

format:
	cargo fmt

format-check:
	cargo fmt -- --check

version-check:
	bash scripts/check_version_alignment.sh

check: lint format-check test

ci: check

wasm:
	cargo build --release --target wasm32-wasip2
	cp target/wasm32-wasip2/release/loom_zed.wasm extension.wasm

wasm-debug:
	cargo build --target wasm32-wasip2
	cp target/wasm32-wasip2/debug/loom_zed.wasm extension.wasm

clean:
	cargo clean
