use crate::output::{Level, Output};
use clap::Clap;
use crossterm::tty::IsTty;
use std::borrow::Borrow;
use std::future::Future;
use std::io::Write;

mod log;
mod shell;

#[derive(Debug, Clap)]
#[clap(about, version)]
struct Args {
    #[clap(flatten)]
    global_options: GlobalOptions,

    #[clap(subcommand)]
    subcommand: Subcommand,
}

#[derive(Debug, Clap)]
enum Subcommand {
    #[clap(aliases = &["tail","logs","syslog"])]
    Log(log::Log),
    Shell(shell::Shell),
}

#[derive(Debug, Clap)]
pub struct GlobalOptions {
    /// Print more information
    #[clap(short, long)]
    verbose: bool,

    /// Print less information
    #[clap(short, long)]
    quiet: bool,
}

#[derive(Debug)]
pub struct Context {
    stdin: std::io::Stdin,
    stdout: std::io::Stdout,
    is_tty: bool,
    global_options: GlobalOptions,
}

impl Context {
    fn new(global_options: GlobalOptions) -> Self {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let is_tty = stdout.is_tty();

        Self {
            stdin,
            stdout,
            is_tty,
            global_options,
        }
    }

    pub fn output<O: Output, V: Borrow<O>>(
        &mut self,
        output: V,
    ) -> Result<(), crossterm::ErrorKind> {
        let output = output.borrow();
        if output.level() > self.global_options.level() {
            return Ok(());
        }

        if self.is_tty {
            output.print(&mut self.stdout)?;
        } else {
            serde_json::to_writer(&mut self.stdout, output)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            self.stdout.write(b"\n")?;
        }

        self.stdout.flush()?;

        Ok(())
    }
}

impl GlobalOptions {
    fn level(&self) -> Level {
        match (self.verbose, self.quiet) {
            (true, false) => crate::output::Level::Debug,
            (false, true) => crate::output::Level::Error,
            _ => crate::output::Level::Info,
        }
    }
}

pub fn main() {
    let Args {
        global_options,
        subcommand,
    } = Args::parse();

    let mut context = Context::new(global_options);

    match subcommand {
        Subcommand::Log(c) => run(c.invoke(&mut context)),
        Subcommand::Shell(c) => run(c.invoke(&mut context)),
    }
}

fn run<E: std::error::Error, F: Future<Output = Result<(), E>>>(future: F) {
    let mut rt = tokio::runtime::Runtime::new().expect("runtime creation failed");

    match rt.block_on(future) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
