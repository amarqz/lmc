.PHONY: all build test clean run check fmt lint install

all: build

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

check:
	cargo check

fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings

run:
	cargo run

install:
	cargo install --path .

clean:
	cargo clean
