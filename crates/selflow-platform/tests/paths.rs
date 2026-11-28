use selflow_platform::paths::{home_dir_from, socket_path_from, store_path_from};

#[test]
fn the_store_lives_under_xdg_state_home_or_home() {
    assert_eq!(
        store_path_from(Some("/s".as_ref()), Some("/h".as_ref())),
        Some("/s/selflow/selflow.db".into())
    );
    assert_eq!(
        store_path_from(Some("".as_ref()), Some("/h".as_ref())),
        Some("/h/.local/state/selflow/selflow.db".into())
    );
    assert_eq!(store_path_from(None, None), None);
}

#[test]
fn the_socket_lives_under_the_runtime_dir() {
    assert_eq!(
        socket_path_from(Some("/run/user/1000".as_ref())),
        Some("/run/user/1000/selflow/daemon.sock".into())
    );
    assert_eq!(socket_path_from(None), None);
}

#[test]
fn the_home_comes_from_a_non_empty_home_variable() {
    assert_eq!(
        home_dir_from(Some("/home/u".as_ref())),
        Some("/home/u".into())
    );
    assert_eq!(home_dir_from(None), None);
    assert_eq!(home_dir_from(Some("".as_ref())), None);
}
