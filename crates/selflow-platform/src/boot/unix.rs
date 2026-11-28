use std::fs;
use std::io;

const BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";

pub fn boot_id() -> io::Result<String> {
    Ok(fs::read_to_string(BOOT_ID_PATH)?.trim().to_owned())
}
