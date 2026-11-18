use notify_rust::{Notification, Urgency};

pub struct Notifier;

impl Notifier {
    pub fn send(&self, title: &str, body: &str) {
        let title = title.to_owned();
        let body = body.to_owned();
        tokio::task::spawn_blocking(move || {
            let shown = Notification::new()
                .appname("Selflow")
                .summary(&title)
                .body(&body)
                .urgency(Urgency::Normal)
                .show();
            if let Err(error) = shown {
                tracing::warn!(%error, "couldn't show a notification");
            }
        });
    }
}
