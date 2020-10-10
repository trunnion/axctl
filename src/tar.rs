use std::borrow::Borrow;
use std::time::SystemTime;

pub fn build<'a, I: IntoIterator<Item = F>, F: Borrow<File>>(files: I) -> Vec<u8> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut gz = deflate::write::GzEncoder::new(
        std::io::Cursor::new(&mut bytes),
        deflate::Compression::Default,
    );
    let mut tar = ::tar::Builder::new(&mut gz);

    for file in files {
        let file = file.borrow();

        let mut header = tar::Header::new_gnu();
        header.set_path(&file.path).unwrap();
        header.set_mode(file.mode);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
        header.set_size(file.bytes.len() as _);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();

        tar.append(&header, std::io::Cursor::new(&file.bytes))
            .unwrap();
    }

    tar.finish().unwrap();
    std::mem::drop(tar);
    gz.finish().unwrap();

    bytes.shrink_to_fit();
    bytes
}

pub fn file<P: AsRef<str>, B: AsRef<[u8]>>(path: P, bytes: B) -> File {
    file_with_mode(path, bytes, 0o644)
}

pub fn executable<P: AsRef<str>, B: AsRef<[u8]>>(path: P, bytes: B) -> File {
    file_with_mode(path, bytes, 0o755)
}

pub fn file_with_mode<P: AsRef<str>, B: AsRef<[u8]>>(path: P, bytes: B, mode: u32) -> File {
    File {
        path: path.as_ref().to_owned(),
        bytes: bytes.as_ref().to_vec(),
        mode,
    }
}

#[derive(Debug, Clone)]
pub struct File {
    path: String,
    bytes: Vec<u8>,
    mode: u32,
}
