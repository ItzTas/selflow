.PHONY: check test install

WINDOWS_TARGET := x86_64-pc-windows-msvc

check:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test
	@test -d "$$(rustc --print sysroot)/lib/rustlib/$(WINDOWS_TARGET)" || { echo "make check needs the $(WINDOWS_TARGET) target: rustup target add --toolchain $$(basename "$$(rustc --print sysroot)") $(WINDOWS_TARGET)" >&2; exit 1; }
	cargo check --target $(WINDOWS_TARGET) -p selflow-core -p selflow-i18n -p selflow-protocol
	! cargo tree -p selflow -e normal --prefix none | grep -q '^iced'

test:
	cargo test

install:
	cargo install --locked --path crates/selflowd
	cargo install --locked --path crates/selflow
	cargo install --locked --path crates/selflow-ui
	install -Dm644 contrib/selflowd.service ~/.config/systemd/user/selflowd.service
	systemctl --user daemon-reload
