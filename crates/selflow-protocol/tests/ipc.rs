use selflow_protocol::ipc::{
    ChangeView, ExtensionSource, MAX_LINE_BYTES, MessageView, RefusalView, Request, Response,
    read_message, write_message,
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

#[test]
fn an_extension_request_without_a_source_comes_from_the_screen() {
    let line = r#"{"type":"request_extension","reason":"talking to my sister","phrase":""}"#;
    assert_eq!(
        serde_json::from_str::<Request>(line).unwrap(),
        Request::RequestExtension {
            reason: "talking to my sister".into(),
            phrase: String::new(),
            source: ExtensionSource::Screen,
        }
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

#[tokio::test]
async fn round_trips_a_test_lock_request_and_its_answer() {
    let (mut writer, other_end) = tokio::io::duplex(64 * 1024);
    let mut reader = tokio::io::BufReader::new(other_end);
    let requests = [
        Request::TestLock {
            rule: Some("every-night/bedtime".into()),
            duration: jiff::SignedDuration::from_mins(5),
        },
        Request::TestLock {
            rule: None,
            duration: jiff::SignedDuration::from_secs(90),
        },
    ];
    for sent in requests {
        write_message(&mut writer, &sent).await.unwrap();
        assert_eq!(
            read_message::<Request>(&mut reader).await.unwrap(),
            Some(sent)
        );
    }
    let sent = Response::TestLockStarted {
        label: "Bedtime".into(),
        ends_at: jiff::Timestamp::from_second(1_000).unwrap(),
    };
    write_message(&mut writer, &sent).await.unwrap();
    assert_eq!(
        read_message::<Response>(&mut reader).await.unwrap(),
        Some(sent)
    );
}

#[tokio::test]
async fn round_trips_a_save_answer_with_each_change_and_its_time() {
    let (mut writer, other_end) = tokio::io::duplex(64 * 1024);
    let mut reader = tokio::io::BufReader::new(other_end);
    let saved_at = jiff::Timestamp::from_second(1_000).unwrap();
    let change = |id: &str, effective_at| ChangeView {
        message: MessageView {
            id: id.into(),
            args: vec![("rule".into(), "bedtime".into())],
        },
        effective_at,
    };
    let sent = Response::Saved {
        saved_at,
        changes: vec![
            change("change-rule-starts-earlier", saved_at),
            change(
                "change-more-extensions",
                jiff::Timestamp::from_second(87_400).unwrap(),
            ),
        ],
    };
    write_message(&mut writer, &sent).await.unwrap();
    assert_eq!(
        read_message::<Response>(&mut reader).await.unwrap(),
        Some(sent)
    );
}
