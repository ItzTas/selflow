#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::boot_id;
