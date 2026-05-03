//! Socket-backed logging for the CLI child.
//!
//! Bridges `env_logger` output from the child process into structured `CliToHost::Log`
//! frames so the host can forward child logs with stable targets and levels.

use std::{
    io::{self, Write},
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
};

use catan_agents::remote_agent::{CliToHost, RemoteLogLevel, write_frame};

struct SocketLogWriter {
    stream: Arc<Mutex<UnixStream>>,
}

impl Write for SocketLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let raw = String::from_utf8_lossy(buf);
        let (level, target, message) = parse_socket_log_line(&raw);
        let mut stream = self
            .stream
            .lock()
            .map_err(|_| io::Error::other("CLI log socket mutex poisoned"))?;
        write_frame(
            &mut *stream,
            &CliToHost::Log {
                level,
                target,
                message,
            },
        )?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut stream = self
            .stream
            .lock()
            .map_err(|_| io::Error::other("CLI log socket mutex poisoned"))?;
        stream.flush()
    }
}

pub(crate) fn init_socket_logger(stream: UnixStream) {
    let writer = SocketLogWriter {
        stream: Arc::new(Mutex::new(stream)),
    };
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.target(env_logger::Target::Pipe(Box::new(writer)));
    builder.format(|buf, record| {
        writeln!(
            buf,
            "{}\t{}\t{}",
            record.level(),
            record.target(),
            record.args()
        )
    });
    if let Err(err) = builder.try_init() {
        eprintln!("failed to initialize CLI socket logger: {err}");
    }
}

pub(crate) fn parse_socket_log_line(raw: &str) -> (RemoteLogLevel, String, String) {
    let raw = raw.trim_end_matches(['\r', '\n']);
    let mut parts = raw.splitn(3, '\t');
    let level = parts
        .next()
        .and_then(parse_remote_log_level)
        .unwrap_or(RemoteLogLevel::Info);
    let target = parts
        .next()
        .unwrap_or("catan_runtime::cli_child")
        .to_owned();
    let message = parts.next().unwrap_or(raw).to_owned();
    (level, target, message)
}

fn parse_remote_log_level(raw: &str) -> Option<RemoteLogLevel> {
    match raw {
        "ERROR" => Some(RemoteLogLevel::Error),
        "WARN" => Some(RemoteLogLevel::Warn),
        "INFO" => Some(RemoteLogLevel::Info),
        "DEBUG" => Some(RemoteLogLevel::Debug),
        "TRACE" => Some(RemoteLogLevel::Trace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use catan_agents::remote_agent::RemoteLogLevel;

    use super::parse_socket_log_line;

    #[test]
    fn socket_log_line_preserves_level_target_and_message() {
        let (level, target, message) =
            parse_socket_log_line("TRACE\tcatan_runtime::cli_child\tselected road\n");

        assert_eq!(level, RemoteLogLevel::Trace);
        assert_eq!(target, "catan_runtime::cli_child");
        assert_eq!(message, "selected road");
    }
}
