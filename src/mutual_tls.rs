//! Key generation for mutual TLS.

use openssl::asn1::Asn1Time;
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private};
use openssl::rsa::Rsa;
use openssl::x509::*;
use std::time::SystemTime;

pub struct Endpoint {
    pub key: Rsa<Private>,
    pub certificate: X509,
    pub peer_certificate_authority: X509,
}

impl Endpoint {
    pub fn ssl_client_connector(&self) -> openssl::ssl::SslConnector {
        use openssl::ssl::*;
        use openssl::x509::store::*;

        let mut builder =
            SslConnector::builder(SslMethod::tls_client()).expect("error creating SslConnector");

        builder
            .set_certificate(&self.certificate)
            .expect("error setting certificate");

        let key = PKey::from_rsa(self.key.clone()).expect("error creating PKey");
        builder
            .set_private_key(&key)
            .expect("error setting private key");

        let ca = {
            let mut b = X509StoreBuilder::new().expect("error creating X509 store");
            b.add_cert(self.peer_certificate_authority.clone())
                .expect("error adding certificate to store");
            b.build()
        };
        builder.set_cert_store(ca);

        builder.build()
    }
}

pub struct Pair {
    pub server: Endpoint,
    pub client: Endpoint,
}

impl Pair {
    pub fn new(name: &str) -> Self {
        // make a pair of certificate authorities
        let server_ca = CA::new(&format!("{} server CA", name)).unwrap();
        let client_ca = CA::new(&format!("{} client CA", name)).unwrap();

        // use them to make a pair of clients
        let (server_key, server_cert) = server_ca.new_client(&format!("{} server", name)).unwrap();
        let (client_key, client_cert) = client_ca.new_client(&format!("{} client", name)).unwrap();

        Pair {
            server: Endpoint {
                key: server_key,
                certificate: server_cert,
                peer_certificate_authority: client_ca.certificate,
            },
            client: Endpoint {
                key: client_key,
                certificate: client_cert,
                peer_certificate_authority: server_ca.certificate,
            },
        }
    }
}

struct CA {
    key: PKey<Private>,
    certificate: X509,
    name: X509Name,
    not_before: Asn1Time,
    not_after: Asn1Time,
}

impl CA {
    fn new(common_name: &str) -> Result<Self, openssl::error::ErrorStack> {
        // make a key
        let key = PKey::from_rsa(new_rsa_key())?;

        // make a CSR
        let mut cert = X509Builder::new()?;
        cert.set_pubkey(&key)?;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("the current time must be after the UNIX epoch")
            .as_secs() as i64;
        let not_before = Asn1Time::from_unix(now - 30 * 86400)?;
        let not_after = Asn1Time::from_unix(now + 30 * 86400)?;
        cert.set_not_before(&not_before)?;
        cert.set_not_after(&not_after)?;

        let name = {
            let mut b = X509NameBuilder::new().unwrap();
            b.append_entry_by_text("CN", common_name).unwrap();
            b.build()
        };
        cert.set_subject_name(&name)?;
        cert.set_issuer_name(&name)?;
        cert.sign(&key, MessageDigest::sha1())?;

        let certificate = cert.build();

        Ok(Self {
            key,
            certificate,
            name,
            not_before,
            not_after,
        })
    }

    fn new_client(
        &self,
        common_name: &str,
    ) -> Result<(Rsa<Private>, X509), openssl::error::ErrorStack> {
        // make a key
        let key = PKey::from_rsa(new_rsa_key())?;

        // make a certificate
        let mut cert = X509Builder::new()?;
        cert.set_pubkey(&key)?;

        cert.set_not_before(&self.not_before)?;
        cert.set_not_after(&self.not_after)?;

        let name = {
            let mut b = X509NameBuilder::new().unwrap();
            b.append_entry_by_text("CN", common_name).unwrap();
            b.build()
        };
        cert.set_subject_name(&name)?;
        cert.set_issuer_name(&self.name)?;
        cert.sign(&self.key, MessageDigest::sha1())?;

        let certificate = cert.build();

        Ok((key.rsa()?, certificate))
    }
}

fn new_rsa_key() -> Rsa<Private> {
    Rsa::generate(2048).unwrap()
}
