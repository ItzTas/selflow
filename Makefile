.PHONY: check test

check:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test

test:
	cargo test
