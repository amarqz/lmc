.PHONY: all build test clean run check fmt lint install debug

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

debug:
	@mkdir -p aux
	LMC_DB_PATH=aux/debug.db cargo run -- $(ARGS)

clean:
	cargo clean
