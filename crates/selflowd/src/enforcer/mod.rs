mod lock_supervisor;
mod notifier;
mod power;

use selflow_protocol::ipc::LockView;
use tokio::sync::watch;

use crate::agent::Directives;
use crate::engine::Enforcer;

pub use lock_supervisor::{LockSupervisor, lock_program_from};
pub use notifier::Notifier;
pub use power::{PowerMode, power_mode, power_mode_from, shut_down};

pub trait ScreenControl: Send + Sync {
    fn release(&self);
}

pub struct SystemEnforcer {
    supervisor: LockSupervisor,
    notifier: Notifier,
    power: PowerMode,
    directives: watch::Sender<Directives>,
}

impl SystemEnforcer {
    pub fn new(supervisor: LockSupervisor, notifier: Notifier, power: PowerMode) -> Self {
        SystemEnforcer {
            supervisor,
            notifier,
            power,
            directives: watch::Sender::new(Directives::default()),
        }
    }

    pub fn views(&self) -> watch::Receiver<Option<LockView>> {
        self.supervisor.views()
    }

    pub fn directives(&self) -> watch::Receiver<Directives> {
        self.directives.subscribe()
    }
}

impl Enforcer for SystemEnforcer {
    fn warn(&mut self, title: String, body: String) {
        self.notifier.send(&title, &body);
    }

    fn show_lock(&mut self, view: LockView) {
        self.supervisor.show(view);
    }

    fn hide_lock(&mut self) {
        self.supervisor.hide();
    }

    fn shut_down(&mut self) {
        power::shut_down(self.power);
    }

    fn direct_agents(&mut self, directives: Directives) {
        self.directives.send_if_modified(|current| {
            if *current == directives {
                return false;
            }
            *current = directives;
            true
        });
    }
}
