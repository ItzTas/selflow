use selflowd::args::asks_for_protocol_version;

fn args<const N: usize>(values: [&str; N]) -> impl Iterator<Item = String> {
    values.into_iter().map(str::to_owned)
}

#[test]
fn only_a_lone_protocol_version_flag_asks_for_the_version() {
    assert!(asks_for_protocol_version(args(["--protocol-version"])));
    assert!(!asks_for_protocol_version(args([])));
    assert!(!asks_for_protocol_version(args(["--lock"])));
    assert!(!asks_for_protocol_version(args([
        "--protocol-version",
        "--lock"
    ])));
}
