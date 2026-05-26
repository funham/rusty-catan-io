use std::{
    io::{self, Read, Write},
    marker::PhantomData,
};

use serde::{Serialize, de::DeserializeOwned};

pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

pub fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(io::Error::other)?;
    if payload.len() > MAX_FRAME_LEN {
        return Err(frame_too_large());
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)
}

pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    let mut header = [0_u8; 4];
    reader.read_exact(&mut header)?;
    let len = u32::from_be_bytes(header) as usize;
    if len > MAX_FRAME_LEN {
        return Err(frame_too_large());
    }
    let mut payload = vec![0_u8; len];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload).map_err(io::Error::other)
}

pub struct NonblockingFrameReader<T> {
    buffer: Vec<u8>,
    _marker: PhantomData<T>,
}

impl<T> Default for NonblockingFrameReader<T> {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            _marker: PhantomData,
        }
    }
}

impl<T: DeserializeOwned> NonblockingFrameReader<T> {
    pub fn poll(&mut self, reader: &mut impl Read) -> io::Result<Option<T>> {
        let mut chunk = [0; 8192];
        let mut eof = false;
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }

        if self.buffer.len() < 4 {
            if eof {
                return Err(unexpected_eof());
            }
            return Ok(None);
        }
        let len = u32::from_be_bytes(
            self.buffer[..4]
                .try_into()
                .expect("slice length checked above"),
        ) as usize;
        if len > MAX_FRAME_LEN {
            return Err(frame_too_large());
        }
        let frame_len = 4 + len;
        if self.buffer.len() < frame_len {
            if eof {
                return Err(unexpected_eof());
            }
            return Ok(None);
        }

        let payload = self.buffer[4..frame_len].to_vec();
        self.buffer.drain(..frame_len);
        serde_json::from_slice(&payload)
            .map(Some)
            .map_err(io::Error::other)
    }
}

fn frame_too_large() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "frame exceeds maximum length")
}

fn unexpected_eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "remote closed frame stream")
}

#[cfg(test)]
mod tests {
    use super::{MAX_FRAME_LEN, NonblockingFrameReader, read_frame, write_frame};

    #[test]
    fn frame_round_trip() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &"hello").unwrap();

        let value: String = read_frame(&mut bytes.as_slice()).unwrap();

        assert_eq!(value, "hello");
    }

    #[test]
    fn nonblocking_reader_decodes_consecutive_frames() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &"first").unwrap();
        write_frame(&mut bytes, &"second").unwrap();
        let mut reader = NonblockingFrameReader::<String>::default();

        let first = reader.poll(&mut bytes.as_slice()).unwrap();
        let second = reader.poll(&mut std::io::empty()).unwrap();

        assert_eq!(first.as_deref(), Some("first"));
        assert_eq!(second.as_deref(), Some("second"));
    }

    #[test]
    fn rejects_large_frame() {
        let bytes = ((MAX_FRAME_LEN + 1) as u32).to_be_bytes().to_vec();

        let err = read_frame::<String>(&mut bytes.as_slice()).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
