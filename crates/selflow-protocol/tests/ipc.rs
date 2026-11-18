use selflow_protocol::ipc::{
    MAX_LINE_BYTES, RefusalView, Request, Response, read_message, socket_path_from, write_message,
};

#[tokio::test]
async fn round_trips_messages_as_json_lines() {
    let (mut writer, other_end) = tokio::io::duplex(64 * 1024);
    let mut reader = tokio::io::BufReader::new(other_end);
    let sent = Request::SaveConfig {
        text: "a\nb".into(),
    };
    write_message(&mut writer, &sent).await.unwrap();
    assert_eq!(
        read_message::<Request>(&mut reader).await.unwrap(),
        Some(sent)
    );
}

#[test]
fn tags_messages_with_a_snake_case_type() {
    assert_eq!(
        serde_json::to_string(&Request::Status).unwrap(),
        r#"{"type":"status"}"#
    );
}

#[tokio::test]
async fn rejects_a_line_over_the_limit() {
    let line = format!("\"{}\"\n", "x".repeat(MAX_LINE_BYTES + 1));
    let mut reader = tokio::io::BufReader::new(line.as_bytes());
    let error = read_message::<Request>(&mut reader).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[tokio::test]
async fn rejects_malformed_json_and_reads_none_at_end_of_input() {
    let mut reader = tokio::io::BufReader::new(&b"not json\n"[..]);
    let error = read_message::<Request>(&mut reader).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(read_message::<Request>(&mut reader).await.unwrap(), None);
}

#[tokio::test]
async fn round_trips_extension_refusals() {
    let refusals = [
        RefusalView::NoBudget,
        RefusalView::TooEarly {
            available_at: jiff::Timestamp::from_second(1_000).unwrap(),
        },
    ];
    for refusal in refusals {
        let (mut writer, other_end) = tokio::io::duplex(64 * 1024);
        let mut reader = tokio::io::BufReader::new(other_end);
        let sent = Response::ExtensionRefused(refusal);
        write_message(&mut writer, &sent).await.unwrap();
        assert_eq!(
            read_message::<Response>(&mut reader).await.unwrap(),
            Some(sent)
        );
    }
}

#[test]
fn the_socket_lives_under_the_runtime_dir() {
    assert_eq!(
        socket_path_from(Some("/run/user/1000".as_ref())),
        Some("/run/user/1000/selflow/daemon.sock".into())
    );
    assert_eq!(socket_path_from(None), None);
}
