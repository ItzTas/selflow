.PHONY: check test install

check:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test

test:
	cargo test

install:
	cargo install --locked --path crates/selflowd
	cargo install --locked --path crates/selflow
	install -Dm644 contrib/selflowd.service ~/.config/systemd/user/selflowd.service
	systemctl --user daemon-reload
