use jiff::civil::Date;
use jiff::{SignedDuration, Timestamp};
use selflow_protocol::ipc::{
    AgentKind, AgentSession, ChallengeView, ChangeView, ConfigStatusView, ExtensionSource,
    LockView, MAX_FRAME_BYTES, MessageView, PROTOCOL_VERSION, PositionedMessage, RefusalView,
    ReportEntry, ReportView, Request, Response, StatusView, ToolCall, ToolVerdict, hello_as_client,
    hello_as_daemon, message_reader, read_message, write_message,
};
use serde::Serialize;
use tokio::io::AsyncWriteExt;

fn at(seconds: i64) -> Timestamp {
    Timestamp::from_second(seconds).unwrap()
}

fn message(id: &str) -> MessageView {
    MessageView {
        id: id.into(),
        args: vec![("rule".into(), "bedtime".into())],
    }
}

fn change(id: &str, effective_at: Timestamp) -> ChangeView {
    ChangeView {
        message: message(id),
        effective_at,
    }
}

fn lock() -> LockView {
    LockView {
        label: "Bedtime".into(),
        ends_at: at(2_000),
        extension_available_at: at(1_500),
        extensions_left: 2,
        extension_minutes: 10,
        reason_required: true,
        phrase: Some("I choose to rest".into()),
        shutdown_at: Some(at(1_900)),
        language: "en".into(),
    }
}

fn agent_session(attended: bool, inbox: Option<&str>) -> AgentSession {
    AgentSession {
        agent: AgentKind::ClaudeCode,
        id: "3f6c1d52".into(),
        cwd: "/home/user/project".into(),
        inbox: inbox.map(str::to_owned),
        attended,
    }
}

fn requests() -> Vec<Request> {
    vec![
        Request::Status,
        Request::SubscribeLock,
        Request::RequestExtension {
            reason: "talking to my sister".into(),
            phrase: String::new(),
            source: ExtensionSource::Terminal,
        },
        Request::RequestExtension {
            reason: String::new(),
            phrase: "I choose to rest".into(),
            source: ExtensionSource::Screen,
        },
        Request::GetConfig,
        Request::SaveConfig {
            text: "a\nb".into(),
        },
        Request::ConfigStatus,
        Request::Report {
            logical_date: Some(Date::constant(2026, 11, 28)),
        },
        Request::TestLock {
            rule: Some("every-night/bedtime".into()),
            duration: SignedDuration::from_mins(5),
        },
        Request::TestLock {
            rule: None,
            duration: SignedDuration::from_secs(90),
        },
        Request::ReleaseScreen,
        Request::StartException {
            rule: "bedtime".into(),
        },
        Request::ConfirmException {
            rule: "bedtime".into(),
            reason: "on call tonight".into(),
            phrase: String::new(),
        },
        Request::AgentSessionStarted(agent_session(true, Some("/run/user/1000/cc-socks/42.sock"))),
        Request::AgentSessionEnded {
            agent: AgentKind::ClaudeCode,
            id: "3f6c1d52".into(),
        },
        Request::AgentPrompt(agent_session(false, None)),
        Request::AgentTool {
            session: agent_session(true, None),
            tool: ToolCall::Edit,
        },
        Request::AgentTool {
            session: agent_session(true, None),
            tool: ToolCall::Shell {
                command: "git status".into(),
            },
        },
        Request::AgentTool {
            session: agent_session(true, None),
            tool: ToolCall::Other,
        },
    ]
}

fn responses() -> Vec<Response> {
    vec![
        Response::Status(StatusView {
            language: "en".into(),
            locked: Some(lock()),
            next_lock_at: Some(at(3_000)),
        }),
        Response::Lock(lock()),
        Response::LockEnded,
        Response::ExtensionGranted { until: at(2_600) },
        Response::ExtensionRefused(RefusalView::NoBudget),
        Response::ExtensionRefused(RefusalView::TooEarly {
            available_at: at(1_000),
        }),
        Response::ExtensionRefused(RefusalView::ReasonTooShort { min_chars: 12 }),
        Response::ExtensionRefused(RefusalView::PhraseMismatch),
        Response::Config {
            text: "[rules]\n".into(),
        },
        Response::Saved {
            saved_at: at(1_000),
            changes: vec![
                change("change-rule-starts-earlier", at(1_000)),
                change("change-more-extensions", at(87_400)),
            ],
        },
        Response::Invalid {
            errors: vec![PositionedMessage {
                message: message("invalid-rule"),
                line: 3,
                column: 7,
            }],
        },
        Response::ConfigStatus(ConfigStatusView {
            last_saved_at: Some(at(1_000)),
            pending: vec![change("change-more-extensions", at(87_400))],
        }),
        Response::Report(ReportView {
            logical_date: Date::constant(2026, 11, 28),
            entries: vec![ReportEntry {
                at: at(1_000),
                message: message("lock-started"),
            }],
            pending: vec![],
        }),
        Response::TestLockStarted {
            label: "Bedtime".into(),
            ends_at: at(1_000),
        },
        Response::Released,
        Response::ExceptionChallenge(ChallengeView {
            label: "bedtime".into(),
            available_at: at(1_000),
            reason_required: true,
            phrase: Some("I choose to rest".into()),
            extensions_left: 1,
        }),
        Response::ExceptionGranted {
            label: "bedtime".into(),
        },
        Response::ExceptionRefused(RefusalView::TooEarly {
            available_at: at(1_000),
        }),
        Response::Error(message("internal-error")),
        Response::AgentNoted,
        Response::AgentContext {
            text: Some("Save your work now.".into()),
        },
        Response::AgentContext { text: None },
        Response::AgentVerdict(ToolVerdict::Allow),
        Response::AgentVerdict(ToolVerdict::Deny {
            reason: "Selflow: only git commands now.".into(),
        }),
    ]
}

macro_rules! variants {
    ($enum:ident: $($variant:ident),+ $(,)?) => {
        (
            [$(stringify!($variant)),+],
            |value: &$enum| match value {
                $($enum::$variant { .. } => stringify!($variant)),+
            },
        )
    };
}

type Variants = Vec<(u8, &'static str)>;

fn wire_coverage<T: Serialize, const N: usize>(
    samples: &[T],
    (declared, variant_of): ([&'static str; N], impl Fn(&T) -> &'static str),
) -> (Variants, Variants) {
    let mut sampled = samples
        .iter()
        .map(|sample| (postcard::to_stdvec(sample).unwrap()[0], variant_of(sample)))
        .collect::<Vec<_>>();
    sampled.sort_unstable();
    sampled.dedup();
    let declared = declared
        .into_iter()
        .enumerate()
        .map(|(index, variant)| (u8::try_from(index).unwrap(), variant))
        .collect();
    (sampled, declared)
}

fn refusals() -> Vec<RefusalView> {
    responses()
        .into_iter()
        .filter_map(|response| match response {
            Response::ExtensionRefused(refusal) | Response::ExceptionRefused(refusal) => {
                Some(refusal)
            }
            _ => None,
        })
        .collect()
}

fn sources() -> Vec<ExtensionSource> {
    requests()
        .into_iter()
        .filter_map(|request| match request {
            Request::RequestExtension { source, .. } => Some(source),
            _ => None,
        })
        .collect()
}

fn agent_kinds() -> Vec<AgentKind> {
    requests()
        .into_iter()
        .filter_map(|request| match request {
            Request::AgentSessionStarted(session) | Request::AgentPrompt(session) => {
                Some(session.agent)
            }
            Request::AgentSessionEnded { agent, .. } => Some(agent),
            Request::AgentTool { session, .. } => Some(session.agent),
            _ => None,
        })
        .collect()
}

fn tool_calls() -> Vec<ToolCall> {
    requests()
        .into_iter()
        .filter_map(|request| match request {
            Request::AgentTool { tool, .. } => Some(tool),
            _ => None,
        })
        .collect()
}

fn tool_verdicts() -> Vec<ToolVerdict> {
    responses()
        .into_iter()
        .filter_map(|response| match response {
            Response::AgentVerdict(verdict) => Some(verdict),
            _ => None,
        })
        .collect()
}

fn encoded_hex() -> String {
    let requests = requests()
        .iter()
        .map(|request| postcard::to_stdvec(request).unwrap())
        .collect::<Vec<_>>();
    let responses = responses()
        .iter()
        .map(|response| postcard::to_stdvec(response).unwrap())
        .collect::<Vec<_>>();
    requests
        .into_iter()
        .chain(responses)
        .map(|bytes| {
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
                + "\n"
        })
        .collect()
}

#[tokio::test]
async fn round_trips_every_request_and_response() {
    let (mut writer, other_end) = tokio::io::duplex(64 * 1024);
    let mut reader = message_reader(other_end);
    for sent in requests() {
        write_message(&mut writer, &sent).await.unwrap();
        assert_eq!(
            read_message::<Request>(&mut reader).await.unwrap(),
            Some(sent)
        );
    }
    for sent in responses() {
        write_message(&mut writer, &sent).await.unwrap();
        assert_eq!(
            read_message::<Response>(&mut reader).await.unwrap(),
            Some(sent)
        );
    }
}

#[tokio::test]
async fn reads_none_at_end_of_input() {
    let mut reader = message_reader(&b""[..]);
    assert_eq!(read_message::<Request>(&mut reader).await.unwrap(), None);
}

#[tokio::test]
async fn rejects_a_frame_over_the_limit() {
    let header = u32::try_from(MAX_FRAME_BYTES + 1).unwrap().to_be_bytes();
    let mut reader = message_reader(&header[..]);
    let error = read_message::<Request>(&mut reader).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[tokio::test]
async fn rejects_bytes_that_are_not_a_message() {
    let mut input = 8u32.to_be_bytes().to_vec();
    input.extend_from_slice(&[0xff; 8]);
    let mut reader = message_reader(&input[..]);
    let error = read_message::<Request>(&mut reader).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn changing_the_wire_format_needs_a_new_protocol_version() {
    assert_eq!(
        (PROTOCOL_VERSION, encoded_hex().as_str()),
        (2, include_str!("wire/v2.hex"))
    );
}

#[test]
fn the_samples_cover_every_variant_of_every_wire_enum() {
    let request = variants!(Request:
        Status,
        SubscribeLock,
        RequestExtension,
        GetConfig,
        SaveConfig,
        ConfigStatus,
        Report,
        TestLock,
        ReleaseScreen,
        StartException,
        ConfirmException,
        AgentSessionStarted,
        AgentSessionEnded,
        AgentPrompt,
        AgentTool,
    );
    let response = variants!(Response:
        Status,
        Lock,
        LockEnded,
        ExtensionGranted,
        ExtensionRefused,
        Config,
        Saved,
        Invalid,
        ConfigStatus,
        Report,
        TestLockStarted,
        Released,
        ExceptionChallenge,
        ExceptionGranted,
        ExceptionRefused,
        Error,
        AgentNoted,
        AgentContext,
        AgentVerdict,
    );
    let refusal = variants!(RefusalView: TooEarly, NoBudget, ReasonTooShort, PhraseMismatch);
    let source = variants!(ExtensionSource: Screen, Terminal);
    let agent = variants!(AgentKind: ClaudeCode);
    let tool = variants!(ToolCall: Edit, Shell, Other);
    let verdict = variants!(ToolVerdict: Allow, Deny);
    for (sampled, declared) in [
        wire_coverage(&requests(), request),
        wire_coverage(&responses(), response),
        wire_coverage(&refusals(), refusal),
        wire_coverage(&sources(), source),
        wire_coverage(&agent_kinds(), agent),
        wire_coverage(&tool_calls(), tool),
        wire_coverage(&tool_verdicts(), verdict),
    ] {
        assert_eq!(sampled, declared);
    }
}

#[tokio::test]
async fn a_hello_round_trip_returns_each_sides_version() {
    let (mut client, mut daemon) = tokio::io::duplex(64);
    let (from_daemon, from_client) =
        tokio::join!(hello_as_client(&mut client), hello_as_daemon(&mut daemon));
    assert_eq!(from_daemon.unwrap(), PROTOCOL_VERSION);
    assert_eq!(from_client.unwrap(), PROTOCOL_VERSION);
}

#[tokio::test]
async fn a_hello_with_another_magic_is_refused() {
    let (mut client, mut daemon) = tokio::io::duplex(64);
    client.write_all(b"JSON\x01\0\0\0").await.unwrap();
    let error = hello_as_daemon(&mut daemon).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}
