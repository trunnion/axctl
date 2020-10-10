use crate::mutual_tls::Endpoint;
use uuid::Uuid;

#[derive(Debug)]
pub struct StartPackage {
    id: Uuid,
    port: u16,
    server_pem: Vec<u8>,
    client_ca_pem: Vec<u8>,
}

impl StartPackage {
    pub fn new(id: Uuid, port: u16, server: &Endpoint) -> Self {
        let server_pem = {
            let key = server
                .key
                .private_key_to_pem()
                .expect("error converting private key to PEM");
            let cert = server
                .certificate
                .to_pem()
                .expect("error converting certificate to PEM");
            key.into_iter().chain(cert.into_iter()).collect()
        };

        let client_ca_pem = server
            .peer_certificate_authority
            .to_pem()
            .expect("error converting CA certificate to PEM");

        StartPackage {
            id,
            port,
            server_pem,
            client_ca_pem,
        }
    }

    pub fn into_eap(self) -> Vec<u8> {
        use crate::tar::*;

        build(&[
            file("package.conf", self.package_dot_conf()),
            executable(self.run_sh_filename(), self.run_sh()),
            file("stunnel.conf", self.stunnel_config()),
            file("server.pem", &self.server_pem),
            file("client_ca.pem", &self.client_ca_pem),
        ])
    }

    fn run_sh_filename(&self) -> String {
        format!("run.{}.sh", &self.id)
    }

    fn package_dot_conf(&self) -> Vec<u8> {
        format!(
            r#"
echo 'starting' | logger -t 'trunnion shell {id}'
run=`find /tmp/ -name {run_sh_filename} | head -n1`
if [ -z "$run" ]
then
    echo 'fatal: unable to identify unpack directory' | logger -t 'trunnion shell {id}'
else
    (
        exec $run </dev/null 2>&1 
    ) &
    echo "$run is running as PID $!" | logger -t 'trunnion shell {id}'
fi
sleep 3

false
"#,
            run_sh_filename = self.run_sh_filename(),
            id = &self.id,
        )
        .into_bytes()
    }

    fn run_sh(&self) -> Vec<u8> {
        format!(
            r#"#!/bin/sh
# trunnion shell invocation
id={}
workdir={}
ssl_port={}

cd `dirname $0`

mkdir $workdir
mv server.pem client_ca.pem stunnel.conf $workdir/

export HOME=/root
export PATH=$PATH:/usr/sbin
export PS1=`hostname`'# '
cd

if command -v stunnel >/dev/null
then
  echo 'starting sh-over-SSL via `stunnel` on port '$ssl_port
  stunnel $workdir/stunnel.conf

elif command -v openssl >/dev/null 2>&1
then
  echo 'starting sh-over-SSL via `openssl` on port '$ssl_port

  mkfifo $workdir/c2s
  sh -i <$workdir/c2s 2>&1 | \
      openssl s_server -quiet \
      -port $ssl_port \
      -cert $workdir/server.pem \
      -key $workdir/server.pem \
      -CAfile $workdir/client_ca.pem \
      -Verify 1 \
      -verify_return_error \
      >$workdir/c2s &
else
  echo 'fatal: `stunnel` and `openssl` are not available'
fi

sleep 10
rm -r $workdir

false
"#,
            self.id,
            self.workdir(),
            self.port,
        )
        .into_bytes()
    }

    fn workdir(&self) -> String {
        format!("/tmp/trunnion-shell.{}", self.id)
    }

    fn stunnel_config(&self) -> Vec<u8> {
        let workdir = self.workdir();

        format!(
            r#"
[sh]
accept   = {}
exec     = /bin/sh
execArgs = sh -i
cert     = {}/server.pem
CAfile   = {}/client_ca.pem
verifyChain = yes
"#,
            self.port, &workdir, &workdir,
        )
        .into_bytes()
    }
}
