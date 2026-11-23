pub mod backend;
mod model;
mod style;
mod view;

use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream};
use iced::keyboard::{self, key};
use iced::{Element, Event, Subscription, Task, event, window};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use selflow_client::client::{ClientError, LockFeed, exchange};
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;
use selflow_protocol::ipc::{ExtensionSource, LockView, Request, Response};

pub use model::{ExtensionArea, FormError, LockEffect, LockEvent, LockModel, Screen};

const TICK: Duration = Duration::from_secs(1);
const RECONNECT_DELAY: Duration = Duration::from_secs(1);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("no runtime directory is set")]
    NoRuntimeDir,
    #[error("the session lock failed: {0}")]
    Session(#[from] backend::BackendError),
    #[error("the preview window failed: {0}")]
    Preview(#[from] iced::Error),
}

#[derive(Debug, Clone)]
pub enum Message {
    Lock(LockEvent),
    Close,
}

pub struct LockScreen {
    model: LockModel,
    localizer: Localizer,
    tz: TimeZone,
    socket: Option<PathBuf>,
}

pub fn run_preview() -> Result<(), LockError> {
    iced::application(LockScreen::preview, LockScreen::update, LockScreen::view)
        .title(LockScreen::title)
        .theme(style::theme())
        .subscription(LockScreen::subscription)
        .run()?;
    Ok(())
}

impl LockScreen {
    pub fn session(socket: PathBuf) -> Self {
        LockScreen::new(Some(socket), Language::En)
    }

    fn new(socket: Option<PathBuf>, language: Language) -> Self {
        LockScreen {
            model: LockModel::new(Timestamp::now()),
            localizer: Localizer::new(language),
            tz: TimeZone::system(),
            socket,
        }
    }

    fn preview() -> (Self, Task<Message>) {
        let mut screen = LockScreen::new(None, Language::from_env());
        let task = screen.apply(LockEvent::View(screen.sample_view()));
        (screen, task)
    }

    fn title(&self) -> String {
        self.localizer.text("lock-brand", &[])
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Lock(event) => self.apply(event),
            Message::Close => self.close(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        view::lock_screen(&self.model, &self.localizer, &self.tz).map(Message::Lock)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let source = match &self.socket {
            Some(socket) => {
                Subscription::run_with(socket.clone(), |socket| daemon_events(socket.clone()))
            }
            None => event::listen_with(on_escape),
        };
        Subscription::batch([ticks(), keyboard::listen().filter_map(on_enter), source])
    }

    fn apply(&mut self, event: LockEvent) -> Task<Message> {
        if let LockEvent::View(view) = &event {
            self.follow_language(&view.language);
        }
        match self.model.update(event) {
            LockEffect::Exit => iced::exit(),
            LockEffect::SendExtensionRequest { reason, phrase } => {
                self.request_extension(reason, phrase)
            }
            LockEffect::None | LockEffect::Reconnect => Task::none(),
        }
    }

    fn close(&self) -> Task<Message> {
        if self.socket.is_some() {
            return Task::none();
        }
        iced::exit()
    }

    fn follow_language(&mut self, tag: &str) {
        let language = Language::from_tag(tag).unwrap_or(Language::En);
        if language == self.localizer.language() {
            return;
        }
        self.localizer = Localizer::new(language);
    }

    fn request_extension(&self, reason: String, phrase: String) -> Task<Message> {
        let Some(socket) = &self.socket else {
            return Task::none();
        };
        Task::perform(
            ask_for_extension(socket.clone(), reason, phrase),
            Message::Lock,
        )
    }

    fn sample_view(&self) -> LockView {
        let now = self.model.now();
        LockView {
            label: self.localizer.text("lock-preview-label", &[]),
            ends_at: now + SignedDuration::from_hours(8),
            extension_available_at: now + SignedDuration::from_mins(1),
            extensions_left: 1,
            extension_minutes: 20,
            reason_required: true,
            phrase: None,
            shutdown_at: Some(now + SignedDuration::from_mins(30)),
            language: self.localizer.language().tag().into(),
        }
    }
}

fn ticks() -> Subscription<Message> {
    iced::time::every(TICK).map(|_| Message::Lock(LockEvent::Tick(Timestamp::now())))
}

fn on_enter(event: keyboard::Event) -> Option<Message> {
    let keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(key::Named::Enter),
        ..
    } = event
    else {
        return None;
    };
    Some(Message::Lock(LockEvent::Submit))
}

fn on_escape(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(key::Named::Escape),
        ..
    }) = event
    else {
        return None;
    };
    Some(Message::Close)
}

fn daemon_events(socket: PathBuf) -> impl Stream<Item = Message> {
    iced::stream::channel(8, async move |mut output: mpsc::Sender<Message>| {
        loop {
            if let Err(error) = follow_lock(&socket, &mut output).await {
                tracing::debug!(%error, "the daemon connection ended");
            }
            let _ = output.send(Message::Lock(LockEvent::Disconnected)).await;
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    })
}

async fn follow_lock(socket: &Path, output: &mut mpsc::Sender<Message>) -> Result<(), ClientError> {
    let mut feed = LockFeed::open(socket).await?;
    while let Some(response) = feed.next().await? {
        let Some(event) = lock_event(response) else {
            continue;
        };
        if output.send(Message::Lock(event)).await.is_err() {
            return Ok(());
        }
    }
    Ok(())
}

fn lock_event(response: Response) -> Option<LockEvent> {
    match response {
        Response::Lock(view) => Some(LockEvent::View(view)),
        Response::LockEnded => Some(LockEvent::Ended),
        _ => None,
    }
}

async fn ask_for_extension(socket: PathBuf, reason: String, phrase: String) -> LockEvent {
    let request = Request::RequestExtension {
        reason,
        phrase,
        source: ExtensionSource::Screen,
    };
    let Ok(answer) = tokio::time::timeout(REQUEST_TIMEOUT, exchange(&socket, &request)).await
    else {
        tracing::warn!(
            timeout = ?REQUEST_TIMEOUT,
            "the daemon didn't answer the extension request in time"
        );
        return LockEvent::RequestFailed;
    };
    match answer {
        Ok(Response::ExtensionGranted { .. }) => LockEvent::Granted,
        Ok(Response::ExtensionRefused(refusal)) => LockEvent::Refused(refusal),
        Ok(response) => {
            tracing::warn!(
                ?response,
                "the daemon gave an unexpected answer to the extension request"
            );
            LockEvent::RequestFailed
        }
        Err(error) => {
            tracing::warn!(%error, "the extension request failed");
            LockEvent::RequestFailed
        }
    }
}
