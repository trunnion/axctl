use crate::cli::Context;
use crate::output::Output;
use clap::Clap;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Stdout, Write};
use std::time::Duration;
use thiserror::Error;
use vapix::v3::system_log::{self, *};

/// Print the system log
#[derive(Debug, Clap)]
pub struct Log {
    /// Print at most this many lines
    #[clap(short, long)]
    number: Option<usize>,

    /// Whether to keep following
    #[clap(short, long)]
    follow: bool,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("error writing to terminal: {0}")]
    TerminalError(#[from] crossterm::ErrorKind),
    #[error("error communicating with camera via VAPIX: {0}")]
    VapixError(#[from] vapix::Error),
}

struct Fields {
    timestamp: bool,
    hostname: bool,
    level: bool,
    source: bool,
}

#[derive(Serialize)]
struct Entry<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<Level>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<Source<'a>>,
    message: &'a str,
}

impl<'a> Output for Entry<'a> {
    fn print(&self, stdout: &mut Stdout) -> Result<(), crossterm::ErrorKind> {
        use crossterm::{queue, style::*};

        if let Some(timestamp) = self.timestamp {
            queue!(
                stdout,
                SetForegroundColor(Color::Grey),
                Print(format!("{} ", timestamp)),
            )?;
        }

        if let Some(hostname) = self.hostname {
            queue!(
                stdout,
                SetForegroundColor(Color::DarkGrey),
                Print(hostname),
                Print(" ")
            )?;
        }

        if let Some(level) = self.level {
            queue!(
                stdout,
                SetForegroundColor(match level {
                    Level::Emergency | Level::Alert | Level::Critical | Level::Error => Color::Red,
                    Level::Warning => Color::Yellow,
                    Level::Notice | Level::Info => Color::Green,
                    Level::Debug => Color::DarkGrey,
                    Level::Repeated => Color::White,
                }),
                Print(format!("{} ", level)),
            )?;
        }

        if let Some(source) = &self.source {
            queue!(
                stdout,
                SetForegroundColor(Color::Cyan),
                Print(format!("{} ", source)),
            )?;
        }

        queue!(stdout, ResetColor, Print(self.message), Print("\n"))
    }
}

impl<'a> From<(system_log::Entry<'a>, &'_ Fields)> for Entry<'a> {
    fn from(t: (system_log::Entry<'a>, &'_ Fields)) -> Self {
        let (
            system_log::Entry {
                timestamp,
                hostname,
                level,
                source,
                message,
            },
            fields,
        ) = t;

        let timestamp = if fields.timestamp {
            Some(timestamp)
        } else {
            None
        };
        let hostname = if fields.hostname {
            Some(hostname)
        } else {
            None
        };
        let level = if fields.level { Some(level) } else { None };
        let source = if fields.source && source.is_some() {
            Some(source)
        } else {
            None
        };

        Self {
            timestamp,
            hostname,
            level,
            source,
            message,
        }
    }
}

#[derive(Serialize)]
struct Entries<'a>(Vec<Entry<'a>>);

impl<'a> Output for Entries<'a> {
    fn print(&self, stdout: &mut Stdout) -> Result<(), crossterm::ErrorKind> {
        for entry in &self.0 {
            entry.print(stdout)?;
        }
        Ok(())
    }
}

fn hash(e: &system_log::Entry) -> u64 {
    let mut h = DefaultHasher::new();
    e.hash(&mut h);
    h.finish()
}

impl<'a> Entries<'a> {
    fn new(
        entries: &'a system_log::Entries,
        config: &'_ Fields,
        n: Option<usize>,
        previous: Option<u64>,
    ) -> (Self, Option<u64>) {
        let mut resume_at = None;
        let mut keepers: Vec<Entry> = entries
            .iter()
            .filter_map(|e| e.ok())
            .take(n.unwrap_or(usize::MAX))
            .map(|e| (hash(&e), e))
            .take_while(|(hash, _)| match previous {
                Some(prev) if prev == *hash => false,
                _ => true,
            })
            .map(|(hash, e)| {
                if resume_at.is_none() {
                    resume_at = Some(hash);
                }
                Entry::from((e, config))
            })
            .collect();

        keepers.reverse();
        (Self(keepers), resume_at.or(previous))
    }
}

impl Log {
    pub async fn invoke(&self, context: &mut Context) -> Result<(), Error> {
        let client = context.client();
        let system_log = client.system_log();

        let mut number = self.number;
        let mut previous = None;

        loop {
            // Get the log
            let buffer = system_log.entries().await?;
            let fields = Fields {
                timestamp: true,
                hostname: false,
                level: true,
                source: true,
            };

            let (entries, hash) = Entries::new(&buffer, &fields, number.take(), previous);

            if !entries.0.is_empty() {
                context.output(entries)?;
            }
            previous = hash;

            if self.follow {
                tokio::time::delay_for(Duration::from_millis(500)).await;
                continue;
            } else {
                break;
            }
        }

        Ok(())
    }
}
