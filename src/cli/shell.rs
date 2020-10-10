use self::end_package::EndPackage;
use self::start_package::StartPackage;
use crate::cli::Context;
use crate::mutual_tls;
use crate::output::{Level, Output};
use clap::Clap;
use crossterm::{queue, style::Print};
use futures::future::Either;
use futures::{pin_mut, prelude::*, select_biased};
use serde::Serialize;
use std::io::{Read, Stdout, Write};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;
use thiserror::Error;
use tokio::prelude::*;
use uuid::Uuid;

mod end_package;
mod start_package;

/// Run an interactive shell on an AXIS camera
#[derive(Debug, Clap)]
pub struct Shell {
    /// The camera URL, formatted as http://user:pass@1.2.3.4/
    camera_url: http::uri::Uri,

    /// The camera-side port on which to establish a shell-over-SSL connection
    #[clap(short, long)]
    port: Option<u16>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartMessage {
    id: Uuid,
    shell_addr: SocketAddr,
}

impl Output for StartMessage {
    fn print(&self, stdout: &mut Stdout) -> Result<(), crossterm::ErrorKind> {
        queue!(
            stdout,
            Print(format!(
                " => starting a shell on {} (session {})\n",
                &self.shell_addr, &self.id
            ))
        )?;
        Ok(())
    }

    fn level(&self) -> Level {
        Level::Debug
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectedMessage {
    id: Uuid,
    shell_addr: SocketAddr,
}

impl Output for ConnectedMessage {
    fn print(&self, stdout: &mut Stdout) -> Result<(), crossterm::ErrorKind> {
        queue!(
            stdout,
            Print(format!(
                " => connected to {} (session {})\n",
                &self.shell_addr, &self.id
            ))
        )?;
        Ok(())
    }

    fn level(&self) -> Level {
        Level::Info
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NegotiatedMessage<'a> {
    tls_version: &'a str,
    cipher: Option<&'a str>,
}

impl<'a> Output for NegotiatedMessage<'a> {
    fn print(&self, stdout: &mut Stdout) -> Result<(), crossterm::ErrorKind> {
        queue!(
            stdout,
            Print(format!(
                " => negotiated {}{}{}\n",
                &self.tls_version,
                self.cipher.and(Some(" with cipher ")).unwrap_or(""),
                self.cipher.unwrap_or(""),
            ))
        )?;
        Ok(())
    }

    fn level(&self) -> Level {
        Level::Debug
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleaningUpMessage {
    id: Uuid,
}

impl Output for CleaningUpMessage {
    fn print(&self, stdout: &mut Stdout) -> Result<(), crossterm::ErrorKind> {
        queue!(
            stdout,
            Print(format!("\n => cleaning up session {}\n", &self.id))
        )?;
        Ok(())
    }

    fn level(&self) -> Level {
        Level::Info
    }
}

async fn ensure_closed(
    addr: SocketAddr,
    timeout: std::time::Duration,
) -> Result<(), std::io::Error> {
    use std::io::{Error, ErrorKind};

    let timeout = tokio::time::delay_for(timeout).fuse();
    let connect = tokio::net::TcpStream::connect(addr).fuse();

    pin_mut!(timeout);
    pin_mut!(connect);

    select_biased! {
        result = connect => {
            match result {
                Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
                    Ok(())
                }
                Ok(_) => Err(Error::new(ErrorKind::AlreadyExists, "destination port is already open")),
                Err(e) => Err(e),
            }
        }
        _ = timeout => {
            Err(Error::new(ErrorKind::TimedOut, "connection timed out (are you behind a firewall?)"))
        }
    }
}

async fn dial(
    addr: SocketAddr,
    timeout: Duration,
) -> Result<tokio::net::TcpStream, std::io::Error> {
    let deadline = tokio::time::delay_for(timeout).fuse();
    pin_mut!(deadline);

    loop {
        let connect = tokio::net::TcpStream::connect(addr).fuse();
        pin_mut!(connect);

        select_biased! {
            result = connect => {
                match result {
                    Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                        // wait just a bit
                        tokio::time::delay_for(Duration::from_millis(100)).await;
                        // loop again
                    }
                    other => {
                        return other
                    }
                }
            }
            _ = deadline => {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "connection timed out"));
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("error writing to terminal: {0}")]
    TerminalError(#[from] crossterm::ErrorKind),
    #[error("error resolving hostname {0:?}: {1}")]
    HostnameResolutionError(String, std::io::Error),
    #[error("error probing {0}: {1}")]
    ProbeError(SocketAddr, std::io::Error),
    #[error("device not supported")]
    DeviceNotSupported,
    #[error("error communicating with camera via VAPIX: {0}")]
    VapixError(vapix::Error<hyper::Error>),
    #[error("failed to start remote shell, check device logs for detail")]
    ShellFailedToStart,
    #[error("failed to connect to remote shell, check device logs for detail: {0}")]
    ShellConnectionError(std::io::Error),
    #[error("TLS handshake failed: {0}")]
    TlsHandshakeFailed(String),
    #[error("error reading from stdin: {0}")]
    InputError(std::io::Error),
    #[error("error writing to stdout: {0}")]
    OutputError(std::io::Error),
    #[error("connection closed: {0}")]
    ConnectionClosed(std::io::Error),
}

impl Shell {
    pub async fn invoke(self, context: &mut Context) -> Result<(), Error> {
        // Pick an ID
        let id = Uuid::new_v4();

        // Make some secrets
        let mutual_tls::Pair { client, server } =
            mutual_tls::Pair::new(&format!("trunnion shell {}", &id));
        let client_connector = client.ssl_client_connector();

        // Pick a port
        let port = self.port.unwrap_or_else(|| choose_port());

        // Resolve the hostname
        let hostname = self
            .camera_url
            .host()
            .expect("URL must have a host component");
        let shell_addr = (hostname, port)
            .to_socket_addrs()
            .map_err(|e| Error::HostnameResolutionError(hostname.to_owned(), e))?
            .next()
            .expect("name resolution produced no addresses");

        // Make a VAPIX device to talk with the camera
        let device = vapix::Device::new(
            vapix::hyper::HyperTransport::default(),
            self.camera_url.clone(),
        );

        // Get the VAPIX applications interface
        // (This also verifies our connectivity, our credentials, etc.)
        let applications = device
            .applications()
            .await
            .map_err(Error::VapixError)?
            .ok_or(Error::DeviceNotSupported)?;

        // Indicate we're about to start
        context.output(StartMessage { id, shell_addr })?;

        // Ensure we promptly get a connection refused on the target port
        ensure_closed(shell_addr, Duration::from_secs(2))
            .await
            .map_err(|e| Error::ProbeError(shell_addr, e))?;

        let conn = {
            // Upload our start package to the camera
            let eap = StartPackage::new(id, port, &server).into_eap();
            let upload_start = async {
                // upload the package
                let result = applications.upload(&eap).await;

                // wait 5 seconds after upload completed for the shell to start
                tokio::time::delay_for(Duration::from_secs(5)).await;

                // now return the result
                result
            }
            .fuse();
            pin_mut!(upload_start);

            // Repeatedly dial the shell port for the next while
            let dial = dial(shell_addr, Duration::from_secs(20)).fuse();
            pin_mut!(dial);

            select_biased! {
                conn = dial => {
                    // We're connected! Or we failed.
                    conn.map_err(Error::ShellConnectionError)
                }
                _ = upload_start => {
                    // If we haven't connected but the upload is completed (and done waiting), then
                    // consider this a failed-to-start situation
                    return Err(Error::ShellFailedToStart);
                }
            }
        }?;

        context.output(ConnectedMessage { id, shell_addr })?;

        // Negotiate TLS
        let conn = {
            // Get a connect config with all the certificates configured
            let mut config = client_connector
                .configure()
                .expect("error getting TLS client config");

            // Don't insist that the CN of the certificate matches our expectations
            config.set_verify_hostname(false);

            // Do the handshake
            tokio_openssl::connect(config, hostname, conn)
                .await
                .map_err(|e| Error::TlsHandshakeFailed(e.to_string()))?
        };

        context.output(NegotiatedMessage {
            tls_version: conn.ssl().version_str(),
            cipher: conn.ssl().current_cipher().map(|cipher| cipher.name()),
        })?;

        // Split into read and write halves
        let (mut conn_read, mut conn_write) = tokio::io::split(conn);

        let c2s = async move {
            tokio::task::spawn(async move {
                let mut buf = [0u8; 1024];
                let mut stdin = std::io::stdin();
                loop {
                    match tokio::task::block_in_place(|| stdin.read(&mut buf)) {
                        Ok(0) => break Ok(()),
                        Ok(n) => {
                            match conn_write.write_all(&buf[0..n]).await {
                                Err(e) => break Err(Error::ConnectionClosed(e)),
                                _ => {}
                            };
                            match conn_write.flush().await {
                                Err(e) => break Err(Error::ConnectionClosed(e)),
                                _ => {}
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break Ok(()),
                        Err(e) => break Err(Error::InputError(e)),
                    }
                }
            })
            .await
            .unwrap()
        };

        let s2c = async {
            let mut buf = [0u8; 1024];
            let mut stdout = tokio::io::stdout();
            loop {
                match conn_read.read(&mut buf).await {
                    Ok(0) => break Ok(()),
                    Ok(n) => {
                        match stdout.write_all(&buf[0..n]).await {
                            Err(e) => break Err(Error::OutputError(e)),
                            _ => {}
                        };
                        match stdout.flush().await {
                            Err(e) => break Err(Error::OutputError(e)),
                            _ => {}
                        };
                    }
                    Err(e) => break Err(Error::ConnectionClosed(e)),
                }
            }
        };

        // Wait until either c2s ends normally or s2c hits an error
        pin_mut!(s2c);
        pin_mut!(c2s);
        let result: Result<(), Error> = match future::try_select(c2s, s2c).await {
            Ok(_) => Ok(()),
            Err(Either::Left((e, _))) | Err(Either::Right((e, _))) => Err(e),
        };

        context.output(CleaningUpMessage { id })?;

        // Clean up on a best-effort basis
        let eap = EndPackage::new(id).into_eap();
        let _ = applications.upload(&eap).await;

        return result;
    }
}

fn choose_port() -> u16 {
    use rand::Rng;

    // 32768-60999 is the typical ephemeral port range
    // We can't guarantee that any of these are available but it's a defensible default
    rand::thread_rng().gen_range(32768, 60999)
}
