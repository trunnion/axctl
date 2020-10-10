use serde::Serialize;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Level {
    Error,
    Info,
    Debug,
}

pub trait Output: Serialize {
    fn print(&self, stdout: &mut std::io::Stdout) -> Result<(), crossterm::ErrorKind>;
    fn level(&self) -> Level {
        Level::Info
    }
}
