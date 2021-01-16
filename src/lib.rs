pub mod ptr;

#[derive(Debug)]
pub enum Error {
    TelemetryNotPresent,
    EventNotPresent,
    ViewCreationFailed,
    UnknownVarType(i32),
    WaitFailed(u32),
}
