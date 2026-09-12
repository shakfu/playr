.PHONY: all build test fmt clippy run clean

all: build

build:
	cargo build

test:
	cargo test
	cargo test --features opus

fmt:
	cargo fmt

clippy:
	cargo clippy --all-targets -- -D warnings
	cargo clippy --all-targets --features opus -- -D warnings

run:
	cargo run --release

clean:
	cargo clean
