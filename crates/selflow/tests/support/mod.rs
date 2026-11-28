#![allow(dead_code)]

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use jiff::civil::date;
use jiff::tz::{self, TimeZone};
use jiff::{Timestamp, ZonedDateTime};
use selflow::editor::Editor;
use selflow::prompt::Prompt;
use selflow_client::client::{Client, DaemonService};
use selflow_platform::transport::listen;
use selflow_protocol::ipc::{
    Request, Response, hello_as_daemon, message_reader, read_message, write_message,
};
use tempfile::TempDir;

pub fn brt() -> TimeZone {
    TimeZone::fixed(tz::offset(-3))
}

pub fn at(day: i8, hour: i8, minute: i8) -> ZonedDateTime {
    date(2026, 11, day)
        .at(hour, minute, 0, 0)
        .to_zoned_datetime(brt())
        .unwrap()
}

pub struct ScriptedEditor {
    answers: VecDeque<String>,
    seen: Vec<String>,
}

impl ScriptedEditor {
    pub fn new<const N: usize>(answers: [&str; N]) -> ScriptedEditor {
        ScriptedEditor {
            answers: answers.into_iter().map(str::to_owned).collect(),
            seen: Vec::new(),
        }
    }

    pub fn seen(&self) -> Vec<String> {
        self.seen.clone()
    }
}

impl Editor for ScriptedEditor {
    fn edit(&mut self, text: &str) -> io::Result<String> {
        self.seen.push(text.to_owned());
        self.answers
            .pop_front()
            .ok_or_else(|| io::Error::other("the script ran out of answers"))
    }
}

pub fn invalid() -> Response {
    Response::Invalid { errors: Vec::new() }
}

pub fn saved() -> Response {
    Response::Saved {
        saved_at: Timestamp::UNIX_EPOCH,
        changes: Vec::new(),
    }
}

pub struct ScriptedPrompt {
    answers: VecDeque<String>,
    said: Vec<String>,
    waits: Vec<Timestamp>,
}

impl ScriptedPrompt {
    pub fn new<const N: usize>(answers: [&str; N]) -> ScriptedPrompt {
        ScriptedPrompt {
            answers: answers.into_iter().map(str::to_owned).collect(),
            said: Vec::new(),
            waits: Vec::new(),
        }
    }

    pub fn said(&self) -> Vec<String> {
        self.said.clone()
    }

    pub fn waits(&self) -> Vec<Timestamp> {
        self.waits.clone()
    }
}

impl Prompt for ScriptedPrompt {
    fn ask(&mut self, _question: &str) -> Option<String> {
        self.answers.pop_front()
    }

    fn say(&mut self, text: &str) {
        self.said.push(text.to_owned());
    }

    fn wait_until(&mut self, at: Timestamp) {
        self.waits.push(at);
    }
}

pub struct FakeDaemon {
    path: PathBuf,
    requests: Arc<Mutex<Vec<Request>>>,
    _dir: TempDir,
}

impl FakeDaemon {
    pub fn new<const N: usize>(responses: [Response; N]) -> FakeDaemon {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("selflow/daemon.sock");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (listening, ready) = mpsc::channel();
        let script = VecDeque::from(responses);
        let socket = path.clone();
        let recorded = Arc::clone(&requests);
        thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(answer(&socket, script, &recorded, listening));
        });
        ready.recv().unwrap();
        FakeDaemon {
            path,
            requests,
            _dir: dir,
        }
    }

    pub fn client(&self) -> Client {
        Client::connect_with(&self.path, &NoRestart).unwrap()
    }

    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

struct NoRestart;

impl DaemonService for NoRestart {
    fn installed_version(&self) -> Option<u32> {
        None
    }

    fn restart(&self) -> bool {
        false
    }
}

async fn answer(
    path: &Path,
    mut script: VecDeque<Response>,
    requests: &Mutex<Vec<Request>>,
    listening: mpsc::Sender<()>,
) {
    let listener = listen(path).unwrap();
    listening.send(()).unwrap();
    while let Ok(mut stream) = listener.accept().await {
        if hello_as_daemon(&mut stream).await.is_err() {
            continue;
        }
        let mut stream = message_reader(stream);
        while let Ok(Some(request)) = read_message::<Request>(&mut stream).await {
            requests.lock().unwrap().push(request);
            let Some(response) = script.pop_front() else {
                break;
            };
            if write_message(stream.get_mut(), &response).await.is_err() {
                break;
            }
        }
    }
}
