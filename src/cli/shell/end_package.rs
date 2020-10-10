use uuid::Uuid;

#[derive(Debug)]
pub(crate) struct EndPackage {
    id: Uuid,
}

impl EndPackage {
    pub fn new(id: Uuid) -> Self {
        EndPackage { id }
    }

    pub fn into_eap(self) -> Vec<u8> {
        use crate::tar::*;

        build(&[file("package.conf", self.package_dot_conf())])
    }

    fn workdir(&self) -> String {
        format!("/tmp/trunnion-shell.{}", self.id)
    }

    fn package_dot_conf(&self) -> Vec<u8> {
        format!(
            r#"(
    workdir={}
    [ -f $workdir/openssl.pid ] && kill `cat $workdir/openssl.pid`
    [ -f $workdir/stunnel.pid ] && kill `cat $workdir/stunnel.pid`
    [ -d $workdir ] && rm -r $workdir
    echo "terminated"
) | logger -t 'trunnion shell {}' &

false
"#,
            &self.workdir(),
            &self.id,
        )
        .into_bytes()
    }
}
