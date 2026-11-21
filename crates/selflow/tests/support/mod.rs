#![allow(dead_code)]

use std::collections::VecDeque;
use std::io;

use jiff::civil::date;
use jiff::tz::{self, TimeZone};
use jiff::{Timestamp, ZonedDateTime};
use selflow::editor::Editor;
use selflow_protocol::ipc::Response;

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
