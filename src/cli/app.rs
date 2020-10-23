use crate::cli::Context;
use crate::output::Output;
use clap::Clap;
use crossterm::ErrorKind;
use serde::Serialize;
use std::io::Stdout;
use thiserror::Error;
use vapix::v3::Applications;

/// Manage installed applications
#[derive(Debug, Clap)]
#[clap(setting = clap::AppSettings::VersionlessSubcommands)]
pub struct App {
    #[clap(subcommand)]
    subcommand: Subcommand,
}

#[derive(Debug, Clap)]
enum Subcommand {
    /// Display information about the application platform
    Info,

    /// List installed applications
    List,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("error writing to terminal: {0}")]
    TerminalError(#[from] crossterm::ErrorKind),
    #[error("VAPIX call failed: {0}")]
    VapixCallFailed(vapix::Error),
    #[error("device not supported, since it does not provide the applications interface")]
    DeviceNotSupported,
}

#[derive(Serialize)]
struct Info {
    firmware_version: Option<String>,
    architecture: Option<vapix::v3::application::Architecture>,
    soc: Option<vapix::v3::application::SOC>,
}

impl<'a, T: vapix::Transport> From<&'a vapix::v3::application::Applications<'a, T>> for Info {
    fn from(a: &'a Applications<'a, T>) -> Self {
        Self {
            architecture: a.architecture(),
            firmware_version: a.firmware_version().map(|str| str.to_owned()),
            soc: a.soc(),
        }
    }
}

impl Output for Info {
    fn print(&self, stdout: &mut Stdout) -> Result<(), ErrorKind> {
        use crossterm::{queue, style::*};
        use std::io::Write;

        fn intensity<T>(o: &Option<T>) -> SetAttribute {
            SetAttribute(if o.is_some() {
                Attribute::Bold
            } else {
                Attribute::Dim
            })
        }

        queue!(
            stdout,
            Print("  Architecture: "),
            intensity(&self.architecture),
            Print(
                &self
                    .architecture
                    .map(|a| a.display_name())
                    .unwrap_or("unknown")
            ),
            SetAttribute(Attribute::NormalIntensity),
            Print("\n"),
            Print("           SOC: "),
            intensity(&self.soc),
            Print(&self.soc.map(|a| a.display_name()).unwrap_or("unknown")),
            SetAttribute(Attribute::NormalIntensity),
            Print("\n"),
            Print("      Firmware: "),
            intensity(&self.firmware_version),
            Print(
                self.firmware_version
                    .as_ref()
                    .map(|a| a.as_ref())
                    .unwrap_or("unknown")
            ),
            SetAttribute(Attribute::NormalIntensity),
            Print("\n"),
        )
    }
}

impl App {
    pub async fn invoke(self, context: &mut Context) -> Result<(), Error> {
        let client = context.client();
        let applications = client
            .applications()
            .await
            .map_err(Error::VapixCallFailed)?
            .ok_or(Error::DeviceNotSupported)?;

        match self.subcommand {
            Subcommand::Info => {
                context.output(Info::from(&applications))?;
            }
            Subcommand::List => todo!(),
        }

        Ok(())
    }
}
