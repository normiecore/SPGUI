//! Owns the Screenpipe process.
//!
//! For a machine that records all day, "the recorder is running" has to be an
//! invariant Deja maintains, not something the user remembers. This module
//! spawns `npx screenpipe record`, watches `/health`, and restarts it with
//! backoff when it dies — unless the user asked for it to stay stopped.

use std::fs::OpenOptions;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::logging;

const TICK: Duration = Duration::from_secs(2);
const WARMUP: Duration = Duration::from_secs(90);
const FIRST_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// User asked for it to stop (or it was never started).
    Stopped,
    /// Process spawned, waiting for `/health`.
    Starting,
    /// `/health` answers.
    Running,
    /// Crashed or unhealthy; waiting to retry.
    Backoff,
    /// Tried too many times; manual intervention needed.
    Failed,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecorderStatus {
    pub phase: Phase,
    pub desired: bool,
    pub pid: Option<u32>,
    pub port: u16,
    /// Empty when healthy.
    pub message: Option<String>,
    pub restarts: u32,
    pub retention_days: i64,
    pub managed: bool,
}

pub struct Recorder {
    pub desired: bool,
    pub phase: Phase,
    pub pid: Option<u32>,
    pub port: u16,
    pub message: Option<String>,
    pub restarts: u32,
    pub retention_days: i64,
    pub managed: bool,
    child: Option<Child>,
    spawned_at: Option<Instant>,
    next_attempt: Option<Instant>,
    backoff: Duration,
    log_path: Option<std::path::PathBuf>,
}

impl Recorder {
    pub fn new(port: u16, retention_days: i64, log_path: Option<std::path::PathBuf>) -> Self {
        Self {
            desired: true,
            phase: Phase::Stopped,
            pid: None,
            port,
            message: None,
            restarts: 0,
            retention_days,
            managed: false,
            child: None,
            spawned_at: None,
            next_attempt: None,
            backoff: FIRST_BACKOFF,
            log_path,
        }
    }

    pub fn status(&self) -> RecorderStatus {
        RecorderStatus {
            phase: self.phase,
            desired: self.desired,
            pid: self.pid,
            port: self.port,
            message: self.message.clone(),
            restarts: self.restarts,
            retention_days: self.retention_days,
            managed: self.managed,
        }
    }

    fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let pid = child.id();
            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            #[cfg(not(windows))]
            {
                let _ = child.kill();
            }
            let _ = child.wait();
            logging::log(format!("recorder: stopped pid {pid}"));
        }
        self.pid = None;
        self.spawned_at = None;
    }

    fn spawn(&mut self) -> Result<(), String> {
        let mut args: Vec<String> = vec![
            "-y".into(),
            "screenpipe@latest".into(),
            "record".into(),
            "--port".into(),
            self.port.to_string(),
        ];
        if self.retention_days > 0 {
            args.push("--retention-days".into());
            args.push(self.retention_days.to_string());
        }

        // npx is a .cmd shim on Windows, so it needs the shell; elsewhere spawn direct.
        #[cfg(windows)]
        let mut cmd = {
            let mut c = Command::new("cmd");
            let mut all = vec!["/C".to_string(), "npx".to_string()];
            all.extend(args);
            c.args(all);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = Command::new("npx");
            c.args(&args);
            c
        };

        cmd.stdin(Stdio::null());

        if let Some(path) = &self.log_path {
            if let Ok(file) = OpenOptions::new().create(true).append(true).open(path) {
                if let Ok(err) = file.try_clone() {
                    cmd.stdout(Stdio::from(file)).stderr(Stdio::from(err));
                }
            }
        }

        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id();
                self.pid = Some(pid);
                self.child = Some(child);
                self.spawned_at = Some(Instant::now());
                self.phase = Phase::Starting;
                self.managed = true;
                self.message = Some("starting the recorder…".into());
                logging::log(format!(
                    "recorder: spawned pid {pid} (retention={}d, port={})",
                    self.retention_days, self.port
                ));
                Ok(())
            }
            Err(err) => {
                self.phase = Phase::Backoff;
                self.message = Some(format!("could not start Screenpipe: {err}"));
                logging::log(format!("recorder: spawn failed: {err}"));
                self.next_attempt = Some(Instant::now() + self.backoff);
                Err(err.to_string())
            }
        }
    }

    fn backoff(&mut self, reason: &str) {
        self.kill();
        self.restarts += 1;
        self.phase = if self.restarts > 8 {
            Phase::Failed
        } else {
            Phase::Backoff
        };
        self.message = Some(reason.to_string());
        self.next_attempt = Some(Instant::now() + self.backoff);
        logging::log(format!(
            "recorder: {} — restart #{} in {}s",
            reason,
            self.restarts,
            self.backoff.as_secs()
        ));
        self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
    }
}

fn health_ok(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(700))
        .timeout(Duration::from_millis(1500))
        .build()
        .get(&url)
        .call()
        .map(|resp| resp.status() == 200)
        .unwrap_or(false)
}

/// Start the watchdog on a background thread.
pub fn start(recorder: Arc<Mutex<Recorder>>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);

        let mut guard = match recorder.lock() {
            Ok(g) => g,
            Err(_) => continue,
        };

        if !guard.desired {
            if guard.child.is_some() {
                guard.kill();
            }
            guard.phase = Phase::Stopped;
            guard.message = None;
            continue;
        }

        let healthy = health_ok(guard.port);

        if healthy {
            guard.phase = Phase::Running;
            guard.message = None;
            guard.backoff = FIRST_BACKOFF;
            guard.restarts = 0;
            continue;
        }

        // Was running, now silently unhealthy — the common case (crashed, or the
        // process was killed). Restart it instead of leaving recording dead.
        if guard.phase == Phase::Running {
            guard.backoff("recorder stopped responding");
            continue;
        }

        // Spawned but not answering yet.
        if guard.phase == Phase::Starting {
            // Detect an early exit instead of waiting the full warmup window.
            let mut died = None;
            if let Some(child) = guard.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    died = Some(format!("recorder exited ({status})"));
                }
            }
            if let Some(reason) = died {
                guard.child = None;
                guard.pid = None;
                guard.backoff(&reason);
                continue;
            }

            if let Some(at) = guard.spawned_at {
                if at.elapsed() < WARMUP {
                    continue;
                }
                guard.backoff("recorder did not come up in 90s");
            }
            continue;
        }

        if guard.phase == Phase::Backoff {
            if let Some(next) = guard.next_attempt {
                if Instant::now() < next {
                    continue;
                }
            }
            guard.next_attempt = None;
            let _ = guard.spawn();
            continue;
        }

        if guard.phase == Phase::Failed {
            continue;
        }

        // Stopped or running-externally but not answering: try to own it.
        if guard.phase == Phase::Stopped {
            guard.phase = Phase::Backoff;
            guard.next_attempt = Some(Instant::now());
        }
    });
}

pub fn stop(recorder: &Arc<Mutex<Recorder>>) {
    if let Ok(mut guard) = recorder.lock() {
        guard.desired = false;
        guard.kill();
        guard.phase = Phase::Stopped;
        guard.message = None;
    }
}

pub fn start_now(recorder: &Arc<Mutex<Recorder>>) {
    if let Ok(mut guard) = recorder.lock() {
        guard.desired = true;
        guard.restarts = 0;
        guard.backoff = FIRST_BACKOFF;
        guard.phase = Phase::Backoff;
        guard.next_attempt = Some(Instant::now());
        guard.message = Some("starting…".into());
    }
}

pub fn restart(recorder: &Arc<Mutex<Recorder>>) {
    if let Ok(mut guard) = recorder.lock() {
        guard.desired = true;
        guard.kill();
        guard.restarts = 0;
        guard.backoff = FIRST_BACKOFF;
        guard.phase = Phase::Backoff;
        guard.next_attempt = Some(Instant::now());
        guard.message = Some("restarting…".into());
    }
}
