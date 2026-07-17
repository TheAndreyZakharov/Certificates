.PHONY: start generate check

start:
	cargo run --release

generate:
	cargo run --release

check:
	cargo check