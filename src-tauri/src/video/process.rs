use crate::{
    domain::{AppError, ErrorCode, Result},
    services::annotation::invalid,
};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn bounded(
    mut reader: impl Read,
    limit: usize,
    overflow: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            return Ok(output);
        }
        if output.len() + count > limit {
            overflow.store(true, Ordering::Relaxed);
            return Err(std::io::Error::other("Tool output exceeded its budget"));
        }
        output.extend_from_slice(&chunk[..count]);
    }
}
pub fn run(
    executable: &Path,
    args: &[String],
    cancel: &AtomicBool,
    limit: usize,
    timeout: Duration,
) -> Result<Vec<u8>> {
    if cancel.load(Ordering::Relaxed) {
        return Err(AppError::new(
            ErrorCode::Cancelled,
            "Video operation cancelled.",
        ));
    }
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let overflow = Arc::new(AtomicBool::new(false));
    let out_limit = overflow.clone();
    let err_limit = overflow.clone();
    let out = std::thread::spawn(move || bounded(stdout, limit, out_limit));
    let err = std::thread::spawn(move || bounded(stderr, 1024 * 1024, err_limit));
    let start = Instant::now();
    let mut failure = None;
    let status = loop {
        if cancel.load(Ordering::Relaxed)
            || overflow.load(Ordering::Relaxed)
            || start.elapsed() > timeout
        {
            failure = Some(if cancel.load(Ordering::Relaxed) {
                AppError::new(
                    ErrorCode::Cancelled,
                    "Video operation cancelled; completed frames remain intact.",
                )
            } else {
                invalid("Video tool timed out or exceeded its output budget.")
            });
            let _ = child.kill();
            break child.wait()?;
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = out
        .join()
        .map_err(|_| invalid("Tool output reader failed."))?;
    let errors = err
        .join()
        .map_err(|_| invalid("Tool error reader failed."))?;
    if let Some(error) = failure {
        return Err(error);
    }
    let output = output?;
    let errors = errors?;
    if !status.success() {
        return Err(invalid(format!(
            "Video tool failed: {}",
            String::from_utf8_lossy(&errors)
                .chars()
                .take(2000)
                .collect::<String>()
        )));
    }
    Ok(output)
}
