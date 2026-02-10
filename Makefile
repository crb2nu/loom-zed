.PHONY: build test lint format format-check check ci clean

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

check: lint format-check test

ci: check

clean:
	cargo clean
