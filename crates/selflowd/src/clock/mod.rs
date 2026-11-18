use jiff::ZonedDateTime;

pub trait Clock: Send + Sync {
    fn now(&self) -> ZonedDateTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> ZonedDateTime {
        ZonedDateTime::now()
    }
}
