use jiff::Timestamp;
use selflow_protocol::ipc::{LockView, RefusalView};

#[derive(Debug, Clone)]
pub enum LockEvent {
    View(LockView),
    Ended,
    Disconnected,
    Tick(Timestamp),
    ReasonEdited(String),
    PhraseEdited(String),
    Submit,
    Granted,
    Refused(RefusalView),
    RequestFailed,
}

#[derive(Debug, PartialEq)]
pub enum LockEffect {
    None,
    SendExtensionRequest { reason: String, phrase: String },
    Exit,
    Reconnect,
}

#[derive(Debug, PartialEq)]
pub enum Screen {
    Connecting,
    Locked {
        label: String,
        until: Timestamp,
        shutdown_at: Option<Timestamp>,
        extension: ExtensionArea,
    },
}

#[derive(Debug, PartialEq)]
pub enum ExtensionArea {
    NoneLeft,
    WaitUntil(Timestamp),
    Form {
        minutes: i64,
        reason_required: bool,
        phrase: Option<String>,
        error: Option<FormError>,
        sending: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FormError {
    Refused(RefusalView),
    Failed,
}

pub struct LockModel {
    now: Timestamp,
    view: Option<LockView>,
    reason: String,
    phrase: String,
    error: Option<FormError>,
    sending: bool,
}

impl LockModel {
    pub fn new(now: Timestamp) -> Self {
        LockModel {
            now,
            view: None,
            reason: String::new(),
            phrase: String::new(),
            error: None,
            sending: false,
        }
    }

    pub fn update(&mut self, event: LockEvent) -> LockEffect {
        match event {
            LockEvent::View(view) => self.view = Some(view),
            LockEvent::Ended => return LockEffect::Exit,
            LockEvent::Disconnected => return LockEffect::Reconnect,
            LockEvent::Tick(now) => self.now = now,
            LockEvent::ReasonEdited(reason) => self.reason = reason,
            LockEvent::PhraseEdited(phrase) => self.phrase = phrase,
            LockEvent::Submit => return self.submit(),
            LockEvent::Granted => {
                self.sending = false;
                self.error = None;
                self.reason.clear();
                self.phrase.clear();
            }
            LockEvent::Refused(refusal) => {
                self.sending = false;
                self.error = Some(FormError::Refused(refusal));
            }
            LockEvent::RequestFailed => {
                self.sending = false;
                self.error = Some(FormError::Failed);
            }
        }
        LockEffect::None
    }

    pub fn screen(&self) -> Screen {
        let Some(view) = &self.view else {
            return Screen::Connecting;
        };
        Screen::Locked {
            label: view.label.clone(),
            until: view.ends_at,
            shutdown_at: view.shutdown_at,
            extension: self.extension(view),
        }
    }

    pub fn now(&self) -> Timestamp {
        self.now
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn phrase(&self) -> &str {
        &self.phrase
    }

    fn extension(&self, view: &LockView) -> ExtensionArea {
        if view.extensions_left == 0 {
            return ExtensionArea::NoneLeft;
        }
        if self.now < view.extension_available_at {
            return ExtensionArea::WaitUntil(view.extension_available_at);
        }
        ExtensionArea::Form {
            minutes: view.extension_minutes,
            reason_required: view.reason_required,
            phrase: view.phrase.clone(),
            error: self.error.clone(),
            sending: self.sending,
        }
    }

    fn submit(&mut self) -> LockEffect {
        let form_open = matches!(
            self.screen(),
            Screen::Locked {
                extension: ExtensionArea::Form { .. },
                ..
            }
        );
        if self.sending || !form_open {
            return LockEffect::None;
        }
        self.sending = true;
        self.error = None;
        LockEffect::SendExtensionRequest {
            reason: self.reason.clone(),
            phrase: self.phrase.clone(),
        }
    }
}
