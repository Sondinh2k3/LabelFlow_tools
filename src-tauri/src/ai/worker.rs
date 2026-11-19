use super::protocol::{self, Model};
use crate::{
    domain::{AppError, ErrorCode, Result},
    services::annotation as a,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub python: PathBuf,
    pub manifest: PathBuf,
}
#[derive(Default)]
pub struct Runtime {
    inner: Mutex<Option<Worker>>,
    generation: AtomicU64,
}
impl Runtime {
    pub fn token(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
    pub fn check(&self, token: u64) -> Result<()> {
        if self.token() != token {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "AI request cancelled. Restart the worker to continue.",
            ));
        }
        Ok(())
    }
    pub fn stop(&self) -> Result<()> {
        self.cancel();
        self.inner
            .lock()
            .map_err(|_| a::invalid("AI state unavailable."))?
            .take();
        Ok(())
    }
    pub fn status(&self) -> Value {
        match self.inner.try_lock() {
            Ok(mut guard) => {
                if guard
                    .as_mut()
                    .is_some_and(|worker| worker.child.try_wait().is_ok_and(|exit| exit.is_some()))
                {
                    guard.take();
                }
                json!({"running":guard.is_some(), "busy":false,
                    "model":guard.as_ref().and_then(|w|w.model.as_ref().map(|m| json!(m)).or_else(|| w.sam.clone()))})
            }
            Err(_) => json!({"running":true,"busy":true,"model":null}),
        }
    }
    pub fn start(&self, config: &Config, directory: &Path, token: u64) -> Result<Value> {
        let mut guard = self
            .inner
            .try_lock()
            .map_err(|_| a::invalid("AI worker is busy."))?;
        guard.take();
        self.check(token)?;
        let script = install(directory)?;
        let mut worker = Worker::spawn(&config.python, &script)?;
        let health = worker.request(
            "health",
            json!({"manifest":config.manifest}),
            self,
            token,
            Duration::from_secs(30),
        )?;
        if health.get("protocol_version").and_then(Value::as_u64) != Some(1)
            || health["provider"] != "CPUExecutionProvider"
            || health["device"] != "cpu"
            || ![
                "health",
                "list_models",
                "load_model",
                "predict_boxes",
                "predict_semantic",
                "cancel",
                "unload",
            ]
            .iter()
            .all(|cap| {
                health["capabilities"]
                    .as_array()
                    .is_some_and(|caps| caps.contains(&json!(cap)))
            })
            || health["missingDependencies"]
                .as_array()
                .is_none_or(|v| !v.is_empty())
        {
            return Err(a::invalid(format!("AI environment is incomplete or incompatible: {health}. Install worker/requirements.txt in the selected Python environment.")));
        }
        let catalog = worker.request(
            "list_models",
            json!({"manifest":config.manifest}),
            self,
            token,
            Duration::from_secs(30),
        )?;
        let value = worker.request(
            "load_model",
            json!({"manifest":config.manifest}),
            self,
            token,
            Duration::from_secs(120),
        )?;
        let model: Model =
            serde_json::from_value(value.clone()).map_err(|e| a::invalid(e.to_string()))?;
        if catalog
            .as_array()
            .is_none_or(|models| models.len() != 1 || models[0] != value)
        {
            return Err(a::invalid(
                "Loaded model disagrees with its declared manifest.",
            ));
        }
        self.check(token)?;
        worker.model = Some(model);
        *guard = Some(worker);
        Ok(json!({"health":health,"model":value}))
    }
    pub fn predict(&self, args: Value, token: u64) -> Result<(Model, Value)> {
        let mut guard = self
            .inner
            .try_lock()
            .map_err(|_| a::invalid("AI worker is busy."))?;
        let worker = guard
            .as_mut()
            .ok_or_else(|| a::invalid("Start the optional AI worker first."))?;
        let model = worker
            .model
            .clone()
            .ok_or_else(|| a::invalid("No AI model loaded."))?;
        let result = worker.request(
            &format!("predict_{}", model.task),
            args,
            self,
            token,
            Duration::from_secs(120),
        );
        match result {
            Ok(value) => Ok((model, value)),
            Err(error) => {
                guard.take();
                Err(error)
            }
        }
    }
    /// Starts a dedicated click-to-mask (SAM) worker. It is a separate process from the
    /// suggestion worker so both models can stay loaded.
    pub fn start_sam(&self, config: &Config, directory: &Path, token: u64) -> Result<Value> {
        let mut guard = self
            .inner
            .try_lock()
            .map_err(|_| a::invalid("SAM worker is busy."))?;
        guard.take();
        self.check(token)?;
        let script = install(directory)?;
        let mut worker = Worker::spawn(&config.python, &script)?;
        let health = worker.request("health", json!({}), self, token, Duration::from_secs(30))?;
        if health["missingDependencies"]
            .as_array()
            .is_none_or(|v| !v.is_empty())
            || !health["capabilities"]
                .as_array()
                .is_some_and(|caps| caps.contains(&json!("sam_predict")))
        {
            return Err(a::invalid(format!("SAM environment is incomplete: {health}. Install worker/requirements.txt in the selected Python environment.")));
        }
        let model = worker.request(
            "sam_load",
            json!({"manifest":config.manifest}),
            self,
            token,
            Duration::from_secs(300),
        )?;
        if model["kind"] != "sam_prompt" {
            return Err(a::invalid("Selected manifest is not a SAM model."));
        }
        self.check(token)?;
        worker.sam = Some(model.clone());
        *guard = Some(worker);
        Ok(model)
    }
    pub fn sam_model(&self) -> Option<Value> {
        self.inner
            .try_lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|w| w.sam.clone()))
    }
    /// One SAM request (encode and/or decode). A failure drops the worker.
    pub fn sam(&self, args: Value, token: u64) -> Result<Value> {
        let mut guard = self
            .inner
            .try_lock()
            .map_err(|_| a::invalid("SAM is busy."))?;
        let worker = guard
            .as_mut()
            .filter(|w| w.sam.is_some())
            .ok_or_else(|| a::invalid("Start SAM first (panel: Click-to-mask)."))?;
        let result = worker.request("sam_predict", args, self, token, Duration::from_secs(180));
        if result.is_err() {
            guard.take();
        }
        result
    }
    pub fn model(&self) -> Result<Model> {
        self.inner
            .try_lock()
            .map_err(|_| a::invalid("AI worker is busy."))?
            .as_ref()
            .and_then(|w| w.model.clone())
            .ok_or_else(|| a::invalid("Start the AI worker first."))
    }
}
pub fn install(directory: &Path) -> Result<PathBuf> {
    let main = include_str!("../../../worker/main.py");
    let adapters = include_str!("../../../worker/adapters.py");
    let sam = include_str!("../../../worker/sam.py");
    let key = a::sha(format!("{main}\n{adapters}\n{sam}").as_bytes());
    let directory = directory.join(format!("worker-{key}"));
    crate::security::reject_symlink(&directory)?;
    fs::create_dir_all(&directory)?;
    for (name, bytes) in [
        ("main.py", main.as_bytes()),
        ("adapters.py", adapters.as_bytes()),
        ("sam.py", sam.as_bytes()),
    ] {
        a::atomic_file(&directory.join(name), bytes)?;
    }
    Ok(directory.join("main.py"))
}

pub fn normalize_python_path(path: &Path) -> Result<PathBuf> {
    let python = path.canonicalize()?;
    if !python.is_file() {
        return Err(a::invalid("Selected Python path is not a file."));
    }
    #[cfg(windows)]
    {
        let is_exe = python
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
        if !is_exe {
            return Err(a::invalid(
                "Choose a Python executable (.exe), for example .cache\\ai-venv\\Scripts\\python.exe.",
            ));
        }
    }
    let probe = Command::new(&python)
        .args(["-I", "-c", "import sys"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match probe {
        Ok(status) if status.success() => Ok(python),
        Ok(status) => Err(a::invalid(format!(
            "Selected Python executable returned exit code {:?}. Choose the environment where worker/requirements.txt is installed.",
            status.code()
        ))),
        Err(error) => {
            #[cfg(windows)]
            if error.raw_os_error() == Some(193) {
                return Err(a::invalid(format!(
                    "Selected file is not a valid Win32 executable: {}. Choose a real Python executable, for example .cache\\ai-venv\\Scripts\\python.exe.",
                    python.display()
                )));
            }
            Err(a::invalid(format!(
                "Cannot start selected Python executable {}: {error}",
                python.display()
            )))
        }
    }
}

pub struct Worker {
    child: Child,
    input: Option<ChildStdin>,
    output: mpsc::Receiver<std::result::Result<Vec<u8>, String>>,
    readers: Vec<thread::JoinHandle<()>>,
    model: Option<Model>,
    sam: Option<Value>,
}
impl Worker {
    pub fn spawn(python: &Path, script: &Path) -> Result<Self> {
        let python = normalize_python_path(python)?;
        let mut command = Command::new(python);
        command
            .args(["-I", "-u", "-B"])
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|error| {
            #[cfg(windows)]
            if error.raw_os_error() == Some(193) {
                return a::invalid("Selected file is not a valid Win32 executable. Choose Python from your virtual environment, for example .cache\\ai-venv\\Scripts\\python.exe.");
            }
            a::invalid(format!("Failed to start Python worker process: {error}"))
        })?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| a::invalid("AI stdin unavailable."))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| a::invalid("AI stdout unavailable."))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| a::invalid("AI stderr unavailable."))?;
        // At most one bounded response is buffered. Never retain arbitrary worker stderr.
        let (sender, output) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let outcome = reader
                    .by_ref()
                    .take((protocol::MAX_MESSAGE + 1) as u64)
                    .read_until(b'\n', &mut line);
                match outcome {
                    Ok(0) => break,
                    Ok(_) if line.len() <= protocol::MAX_MESSAGE && line.last() == Some(&b'\n') => {
                        if sender.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    _ => {
                        let _ =
                            sender.send(Err("AI worker output exceeded protocol limits.".into()));
                        break;
                    }
                }
            }
        });
        let errors = thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });
        Ok(Self {
            child,
            input: Some(input),
            output,
            readers: vec![reader, errors],
            model: None,
            sam: None,
        })
    }
    pub fn request(
        &mut self,
        operation: &str,
        args: Value,
        runtime: &Runtime,
        token: u64,
        timeout: Duration,
    ) -> Result<Value> {
        runtime.check(token)?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut bytes = serde_json::to_vec(&json!({"protocol_version":protocol::VERSION,"requestId":id,"operation":operation,"args":args})).map_err(|e|a::invalid(e.to_string()))?;
        bytes.push(b'\n');
        if bytes.len() > protocol::MAX_MESSAGE {
            return Err(a::invalid("AI request exceeds IPC budget."));
        }
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| a::invalid("AI worker is stopped."))?;
        input.write_all(&bytes).and_then(|()|input.flush()).map_err(|error|a::invalid(format!("AI worker pipe closed: {error}. Restart the worker; manual labels are unchanged.")))?;
        let started = Instant::now();
        loop {
            runtime.check(token)?;
            if started.elapsed() > timeout {
                return Err(a::invalid("AI worker timed out; restart it to continue."));
            }
            match self.output.recv_timeout(Duration::from_millis(25)) {
                Ok(Ok(line)) => {
                    let value = serde_json::from_slice(&line).map_err(|e|a::invalid(format!("Invalid AI JSON: {e}")))?;
                    runtime.check(token)?;
                    return protocol::result(value,&id);
                }
                Ok(Err(error)) => return Err(a::invalid(error)),
                Err(mpsc::RecvTimeoutError::Timeout) => {},
                Err(_) => return Err(a::invalid("AI worker exited unexpectedly. Manual labels are unchanged; restart the worker.")),
            }
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        // Signal EOF first, including to a Windows virtual-environment redirector's interpreter.
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Drop the receiver before joining so an unsolicited second response cannot block shutdown.
        let (_, replacement) = mpsc::channel();
        drop(std::mem::replace(&mut self.output, replacement));
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}
