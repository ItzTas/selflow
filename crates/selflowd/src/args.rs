use selflow_protocol::ipc::PROTOCOL_VERSION_FLAG;

pub fn asks_for_protocol_version(mut args: impl Iterator<Item = String>) -> bool {
    args.next().is_some_and(|arg| arg == PROTOCOL_VERSION_FLAG) && args.next().is_none()
}
