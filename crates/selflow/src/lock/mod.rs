mod model;
mod style;
mod view;

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream};
use iced::keyboard::{self, key};
use iced::{Element, Event, Subscription, Task, event, window};
use iced_exwlshell::to_sessionlock_message;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;
use selflow_protocol::ipc::{LockView, Request, Response, read_message, write_message};
use tokio::io::BufReader;
use tokio::net::UnixStream;

pub use model::{ExtensionArea, LockEffect, LockEvent, LockModel, Screen};

const TICK: Duration = Duration::from_secs(1);
const RECONNECT_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("the session lock failed: {0}")]
    Session(#[from] iced_exwlshell::Error),
    #[error("the preview window failed: {0}")]
    Preview(#[from] iced::Error),
}

pub fn run_session_lock(socket: PathBuf) -> Result<(), LockError> {
    let boot = move || LockScreen::new(Some(socket.clone()), Language::En);
    iced_exwlshell::sessionlock::application(boot, LockScreen::update, LockScreen::view)
        .theme(style::theme())
        .subscription(LockScreen::subscription)
        .run()?;
    Ok(())
}

pub fn run_preview() -> Result<(), LockError> {
    iced::application(LockScreen::preview, LockScreen::update, LockScreen::view)
        .title(LockScreen::title)
        .theme(style::theme())
        .subscription(LockScreen::subscription)
        .run()?;
    Ok(())
}

#[to_sessionlock_message]
#[derive(Debug, Clone)]
enum Message {
    Lock(LockEvent),
    Close,
}

struct LockScreen {
    model: LockModel,
    localizer: Localizer,
    tz: TimeZone,
    daemon: Option<PathBuf>,
}

impl LockScreen {
    fn new(daemon: Option<PathBuf>, language: Language) -> Self {
        LockScreen {
            model: LockModel::new(Timestamp::now()),
            localizer: Localizer::new(language),
            tz: TimeZone::system(),
            daemon,
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

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Lock(event) => self.apply(event),
            Message::Close => self.close(),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        view::lock_screen(&self.model, &self.localizer, &self.tz).map(Message::Lock)
    }

    fn subscription(&self) -> Subscription<Message> {
        let source = match &self.daemon {
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
        if self.daemon.is_some() {
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
        let Some(socket) = self.daemon.clone() else {
            return Task::none();
        };
        Task::perform(ask_for_extension(socket, reason, phrase), Message::Lock)
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

async fn follow_lock(socket: &Path, output: &mut mpsc::Sender<Message>) -> io::Result<()> {
    let mut stream = BufReader::new(UnixStream::connect(socket).await?);
    write_message(stream.get_mut(), &Request::SubscribeLock).await?;
    while let Some(response) = read_message(&mut stream).await? {
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
    let request = Request::RequestExtension { reason, phrase };
    match exchange(&socket, &request).await {
        Ok(Some(Response::ExtensionGranted { .. })) => LockEvent::Granted,
        Ok(Some(Response::ExtensionRefused(refusal))) => LockEvent::Refused(refusal),
        _ => LockEvent::Disconnected,
    }
}

async fn exchange(socket: &Path, request: &Request) -> io::Result<Option<Response>> {
    let mut stream = BufReader::new(UnixStream::connect(socket).await?);
    write_message(stream.get_mut(), request).await?;
    read_message(&mut stream).await
}
