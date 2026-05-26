use std::{io, os::unix::net::UnixStream, path::Path};

pub fn connect(path: &Path) -> io::Result<UnixStream> {
    UnixStream::connect(path)
}
