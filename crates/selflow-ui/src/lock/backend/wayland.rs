use std::path::PathBuf;

use iced::{Element, Subscription, Task};
use iced_exwlshell::shell::{self, ShellEvent, ShellReceiver};
use iced_exwlshell::{ExWlSettings, to_sessionlock_message};

use crate::lock::{LockScreen, Message, style};

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("the Wayland session lock failed: {0}")]
    Wayland(#[from] iced_exwlshell::Error),
}

#[to_sessionlock_message]
#[derive(Debug, Clone)]
enum SessionMessage {
    Screen(Message),
    Lost(ShellEvent),
}

struct SessionLock {
    screen: LockScreen,
    shell: ShellReceiver,
}

pub fn run_session_lock(socket: PathBuf) -> Result<(), BackendError> {
    let (shell_broadcast, shell) = shell::channel();
    let boot = move || SessionLock {
        screen: LockScreen::session(socket.clone()),
        shell: shell.clone(),
    };
    iced_exwlshell::sessionlock::application(boot, SessionLock::update, SessionLock::view)
        .wl_settings(ExWlSettings {
            shell_broadcast,
            ..ExWlSettings::default()
        })
        .theme(style::theme())
        .subscription(SessionLock::subscription)
        .run()?;
    Ok(())
}

impl SessionLock {
    fn update(&mut self, message: SessionMessage) -> Task<SessionMessage> {
        match message {
            SessionMessage::Screen(message) => {
                self.screen.update(message).map(SessionMessage::Screen)
            }
            SessionMessage::Lost(event) => {
                tracing::warn!(
                    ?event,
                    "the compositor denied or ended the session lock; exiting so a new client takes it"
                );
                iced::exit()
            }
        }
    }

    fn view(&self) -> Element<'_, SessionMessage> {
        self.screen.view().map(SessionMessage::Screen)
    }

    fn subscription(&self) -> Subscription<SessionMessage> {
        Subscription::batch([
            self.screen.subscription().map(SessionMessage::Screen),
            self.shell.listen().filter_map(lock_lost),
        ])
    }
}

fn lock_lost(event: ShellEvent) -> Option<SessionMessage> {
    match event {
        ShellEvent::LockDenied | ShellEvent::LockedFinished => Some(SessionMessage::Lost(event)),
        _ => None,
    }
}
