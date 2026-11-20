use std::io;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use selflow_protocol::ipc::LockView;
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, watch};
use tokio::time::Instant;

const RESPAWN_DELAY: Duration = Duration::from_secs(1);
const MAX_RESPAWN_DELAY: Duration = Duration::from_secs(30);
const STEADY_RUN: Duration = Duration::from_secs(5);
const HIDE_GRACE: Duration = Duration::from_secs(5);

pub struct LockSupervisor {
    shared: Arc<Shared>,
}

struct Shared {
    program: PathBuf,
    args: Vec<String>,
    views: watch::Sender<Option<LockView>>,
    running: Mutex<Running>,
}

#[derive(Default)]
struct Running {
    pid: Option<u32>,
    generation: u64,
    stop: Option<oneshot::Sender<()>>,
    next_spawn_at: Option<Instant>,
    quick_exits: u32,
}

impl LockSupervisor {
    pub fn new(program: PathBuf, args: Vec<String>) -> Self {
        let (views, _) = watch::channel(None);
        LockSupervisor {
            shared: Arc::new(Shared {
                program,
                args,
                views,
                running: Mutex::new(Running::default()),
            }),
        }
    }

    pub fn show(&mut self, view: LockView) {
        self.shared.views.send_if_modified(|current| {
            if current.as_ref() == Some(&view) {
                return false;
            }
            *current = Some(view);
            true
        });
        start(&self.shared);
    }

    pub fn hide(&mut self) {
        self.shared
            .views
            .send_if_modified(|current| current.take().is_some());
        let generation = {
            let running = self.shared.running();
            if running.pid.is_none() {
                return;
            }
            running.generation
        };
        let shared = Arc::clone(&self.shared);
        tokio::spawn(async move {
            tokio::time::sleep(HIDE_GRACE).await;
            if shared.views.borrow().is_some() {
                return;
            }
            shared.running().stop(generation);
        });
    }

    pub fn views(&self) -> watch::Receiver<Option<LockView>> {
        self.shared.views.subscribe()
    }

    pub fn pid(&self) -> Option<u32> {
        self.shared.running().pid
    }
}

impl Shared {
    fn running(&self) -> MutexGuard<'_, Running> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Running {
    fn stop(&mut self, generation: u64) {
        if self.generation != generation {
            return;
        }
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }

    fn waiting(&self) -> bool {
        self.next_spawn_at.is_some_and(|at| Instant::now() < at)
    }

    fn back_off(&mut self, ran: Duration) -> Instant {
        if ran >= STEADY_RUN {
            self.quick_exits = 0;
            return self.respawn_in(RESPAWN_DELAY);
        }
        let delay = RESPAWN_DELAY
            .saturating_mul(2u32.saturating_pow(self.quick_exits))
            .min(MAX_RESPAWN_DELAY);
        self.quick_exits = self.quick_exits.saturating_add(1);
        self.respawn_in(delay)
    }

    fn respawn_in(&mut self, delay: Duration) -> Instant {
        let at = Instant::now() + delay;
        self.next_spawn_at = Some(at);
        at
    }
}

pub fn lock_program_from(current_exe: &Path) -> PathBuf {
    let sibling = current_exe.with_file_name("selflow-ui");
    if sibling.exists() {
        return sibling;
    }
    PathBuf::from("selflow-ui")
}

fn start(shared: &Arc<Shared>) {
    let mut running = shared.running();
    if running.pid.is_some() || running.waiting() || shared.views.borrow().is_none() {
        return;
    }
    let spawned = Command::new(&shared.program)
        .args(&shared.args)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn();
    let child = match spawned {
        Ok(child) => child,
        Err(error) => {
            let program = shared.program.display();
            tracing::error!(%error, %program, "couldn't start the lock client");
            retry(shared, running.back_off(Duration::ZERO));
            return;
        }
    };
    let (stop, stopped) = oneshot::channel();
    running.generation += 1;
    running.pid = child.id();
    running.stop = Some(stop);
    tokio::spawn(supervise(
        Arc::clone(shared),
        child,
        stopped,
        Instant::now(),
    ));
}

fn retry(shared: &Arc<Shared>, at: Instant) {
    let shared = Arc::clone(shared);
    tokio::spawn(async move {
        tokio::time::sleep_until(at).await;
        start(&shared);
    });
}

async fn supervise(
    shared: Arc<Shared>,
    mut child: Child,
    stopped: oneshot::Receiver<()>,
    started: Instant,
) {
    let status = tokio::select! {
        status = child.wait() => status,
        Ok(()) = stopped => stop(&mut child).await,
    };
    let ran = started.elapsed();
    log_exit(status, ran);
    if let Some(at) = released(&shared, ran) {
        retry(&shared, at);
    }
}

fn released(shared: &Shared, ran: Duration) -> Option<Instant> {
    let mut running = shared.running();
    running.pid = None;
    running.stop = None;
    shared
        .views
        .borrow()
        .is_some()
        .then(|| running.back_off(ran))
}

async fn stop(child: &mut Child) -> io::Result<ExitStatus> {
    child.start_kill()?;
    child.wait().await
}

fn log_exit(status: io::Result<ExitStatus>, ran: Duration) {
    match status {
        Ok(status) if ran < STEADY_RUN => {
            tracing::warn!(%status, ?ran, "the lock client exited soon after it started");
        }
        Ok(status) => tracing::info!(%status, "the lock client exited"),
        Err(error) => tracing::warn!(%error, "couldn't wait on the lock client"),
    }
}
