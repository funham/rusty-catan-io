use std::{
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

use chrono::{DateTime, Utc};

use crate::config::LoggingConfig;

struct TeeLogWriter {
    stderr: io::Stderr,
    file: Option<File>,
}

impl Write for TeeLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stderr.write_all(buf)?;
        if let Some(file) = &mut self.file {
            file.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stderr.flush()?;
        if let Some(file) = &mut self.file {
            file.flush()?;
        }
        Ok(())
    }
}

pub(crate) fn init_host_logger(config: &LoggingConfig) -> Result<Option<PathBuf>, String> {
    let log_path = if config.enabled {
        let path = timestamped_log_path(config, Utc::now());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!("failed to create log directory {}: {err}", parent.display())
            })?;
        }
        Some(path)
    } else {
        None
    };

    let file = match &log_path {
        Some(path) => Some(
            File::create(path)
                .map_err(|err| format!("failed to create log file {}: {err}", path.display()))?,
        ),
        None => None,
    };

    let writer = TeeLogWriter {
        stderr: io::stderr(),
        file,
    };
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.target(env_logger::Target::Pipe(Box::new(writer)));
    builder
        .try_init()
        .map_err(|err| format!("failed to initialize logger: {err}"))?;

    if let Some(path) = &log_path {
        log::info!("writing runtime logs to {}", path.display());
    }

    Ok(log_path)
}

pub(crate) fn timestamped_log_path(config: &LoggingConfig, timestamp: DateTime<Utc>) -> PathBuf {
    let stamp = timestamp.format("%Y-%m-%dT%H-%M-%SZ");
    config
        .directory
        .join(format!("{}-{stamp}.log", config.file_prefix))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use crate::config::LoggingConfig;

    use super::timestamped_log_path;

    #[test]
    fn default_timestamped_log_path_uses_target_logs() {
        let config = LoggingConfig::default();
        let timestamp = Utc.with_ymd_and_hms(2026, 5, 4, 12, 30, 5).unwrap();

        let path = timestamped_log_path(&config, timestamp);

        assert_eq!(
            path,
            std::path::PathBuf::from("target/catan-logs/rusty-catan-2026-05-04T12-30-05Z.log")
        );
    }

    #[test]
    fn timestamped_log_path_honors_config() {
        let config = LoggingConfig {
            enabled: true,
            directory: "tmp/logs".into(),
            file_prefix: "match".to_owned(),
        };
        let timestamp = Utc.with_ymd_and_hms(2026, 5, 4, 12, 30, 5).unwrap();

        let path = timestamped_log_path(&config, timestamp);

        assert_eq!(
            path,
            std::path::PathBuf::from("tmp/logs/match-2026-05-04T12-30-05Z.log")
        );
    }
}
