use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jiff::ZonedDateTime;
use selflow_platform::transport::connect;
use selflow_protocol::ipc::{AgentKind, AgentSession};
use tokio::io::AsyncWriteExt;
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};

use super::claude_code::inbox_frame;
use super::constants::{COURIER_RESTART_DELAY, INBOX_DELIVERY, INBOX_TIMEOUT, PARAGRAPH_BREAK};
use super::{AgentHub, Directives};
use crate::clock::Clock;

pub async fn supervise_courier(hub: Arc<AgentHub>, clock: Arc<dyn Clock>) {
    restarting(COURIER_RESTART_DELAY, || {
        run_courier(Arc::clone(&hub), Arc::clone(&clock))
    })
    .await;
}

pub async fn restarting<T>(delay: Duration, mut start: impl FnMut() -> T)
where
    T: Future<Output = ()> + Send + 'static,
{
    loop {
        match tokio::spawn(start()).await {
            Ok(()) => tracing::error!("the agent courier stopped, so it starts again"),
            Err(error) => tracing::error!(%error, "the agent courier failed, so it starts again"),
        }
        sleep(delay).await;
    }
}

pub async fn run_courier(hub: Arc<AgentHub>, clock: Arc<dyn Clock>) {
    let mut directives = hub.directives.clone();
    loop {
        let current = directives.borrow_and_update().clone();
        deliver_all(&hub, &current, &clock.now()).await;
        tokio::select! {
            changed = directives.changed() => {
                if changed.is_err() {
                    return;
                }
            }
            () = hub.changed().notified() => {}
        }
    }
}

pub async fn deliver(inbox: &Path, frame: &[u8]) -> io::Result<()> {
    timeout(INBOX_TIMEOUT, write_frame(inbox, frame)).await?
}

async fn write_frame(inbox: &Path, frame: &[u8]) -> io::Result<()> {
    let mut stream = connect(inbox).await?;
    stream.write_all(frame).await?;
    stream.shutdown().await
}

async fn deliver_all(hub: &Arc<AgentHub>, directives: &Directives, now: &ZonedDateTime) {
    if directives.steps.is_empty() {
        return;
    }
    let directives = Arc::new(directives.clone());
    let mut deliveries = JoinSet::new();
    for session in hub.sessions() {
        deliveries.spawn(deliver_to(
            Arc::clone(hub),
            session,
            Arc::clone(&directives),
            now.clone(),
        ));
    }
    deliveries.join_all().await;
}

async fn deliver_to(
    hub: Arc<AgentHub>,
    session: AgentSession,
    directives: Arc<Directives>,
    now: ZonedDateTime,
) {
    let Some(inbox) = session.inbox.clone() else {
        return;
    };
    let at = now.timestamp();
    let claimed = hub.claim_unsent(&session, &directives.steps, INBOX_DELIVERY, at);
    if claimed.is_empty() {
        return;
    }
    let text = hub.with_texts(&directives, now.time_zone(), |texts| {
        claimed
            .iter()
            .map(|step| texts.message(step))
            .collect::<Vec<String>>()
            .join(PARAGRAPH_BREAK)
    });
    let frame = match session.agent {
        AgentKind::ClaudeCode => inbox_frame(&session.id, &text),
    };
    let Err(error) = deliver(Path::new(&inbox), &frame).await else {
        hub.record_sent(&session, &claimed, INBOX_DELIVERY, at);
        return;
    };
    tracing::warn!(%error, inbox, "couldn't write to an agent inbox, so the message waits for the next prompt");
    hub.release(&session, &claimed);
    if matches!(
        error.kind(),
        ErrorKind::NotFound | ErrorKind::ConnectionRefused
    ) {
        hub.forget(&session);
    }
}
