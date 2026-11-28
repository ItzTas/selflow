use selflow_platform::transport::{AcceptError, ListenError, check_peer, connect, listen};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
async fn a_line_goes_both_ways() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let listener = listen(&path).unwrap();
    let server = tokio::spawn(async move {
        let mut stream = BufReader::new(listener.accept().await.unwrap());
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        stream.get_mut().write_all(b"pong\n").await.unwrap();
        line
    });
    let mut client = BufReader::new(connect(&path).await.unwrap());
    client.get_mut().write_all(b"ping\n").await.unwrap();
    let mut answer = String::new();
    client.read_line(&mut answer).await.unwrap();
    assert_eq!(answer, "pong\n");
    assert_eq!(server.await.unwrap(), "ping\n");
}

#[tokio::test]
async fn a_second_listener_is_refused_while_the_first_answers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let _first = listen(&path).unwrap();
    assert!(matches!(listen(&path), Err(ListenError::AlreadyRunning)));
}

#[tokio::test]
async fn a_leftover_socket_file_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "").unwrap();
    assert!(listen(&path).is_ok());
}

#[tokio::test]
async fn a_peer_of_the_same_user_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let listener = listen(&path).unwrap();
    let _client = connect(&path).await.unwrap();
    assert!(listener.accept().await.is_ok());
}

#[test]
fn a_peer_with_the_owner_uid_passes_the_check() {
    assert!(check_peer(1000, 1000).is_ok());
}

#[test]
fn a_peer_with_another_uid_is_refused() {
    assert!(matches!(
        check_peer(1000, 1001),
        Err(AcceptError::ForeignPeer { uid: 1001 })
    ));
}
