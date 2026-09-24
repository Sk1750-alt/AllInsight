//! The local inference engine.
//!
//! AllInsight runs llama.cpp's `llama-server` as a child process bound to loopback
//! and talks to it over HTTP. That choice does three things at once:
//!
//! * inference happens in a separate process, so a model that hangs or runs
//!   out of memory cannot take the interface with it
//! * the engine is swappable - anything that speaks the same small HTTP shape
//!   can be pointed at instead
//! * AllInsight's own binary contains no inference code and no model weights
//!
//! The connection is to `127.0.0.1` and nowhere else. There is no code in this
//! module that can reach a remote host.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{AllInsightError, Result};

/// Ports AllInsight will try, in order. High and unlikely to collide.
const PORT_RANGE: std::ops::Range<u16> = 51_700..51_720;

/// How long to wait for the server to become ready before giving up.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);

/// Cap on a single generation, so a runaway model cannot produce unbounded
/// text into the interface.
const MAX_TOKENS: u32 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    /// No model configured, or the user turned the assistant off.
    Idle,
    Starting,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub state: EngineState,
    pub model_name: Option<String>,
    pub engine_path: Option<PathBuf>,
    pub port: Option<u16>,
    pub message: String,
    /// Always true. Stated explicitly because it is the point.
    pub offline_only: bool,
}

impl Default for EngineStatus {
    fn default() -> Self {
        Self {
            state: EngineState::Idle,
            model_name: None,
            engine_path: None,
            port: None,
            message: "No local model is loaded. AllInsight works fully without one.".into(),
            offline_only: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub engine_path: PathBuf,
    pub model_path: PathBuf,
    pub context_size: u32,
    pub threads: u32,
    pub gpu_layers: u32,
}

pub struct LlamaEngine {
    inner: Mutex<Inner>,
}

struct Inner {
    child: Option<Child>,
    port: Option<u16>,
    /// The credential this engine process was started with. Regenerated on
    /// every launch and never written anywhere.
    api_key: Option<String>,
    status: EngineStatus,
    config: Option<EngineConfig>,
}

impl Default for LlamaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LlamaEngine {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                child: None,
                port: None,
                api_key: None,
                status: EngineStatus::default(),
                config: None,
            }),
        }
    }

    pub fn status(&self) -> EngineStatus {
        self.inner.lock().status.clone()
    }

    pub fn is_ready(&self) -> bool {
        self.inner.lock().status.state == EngineState::Ready
    }

    /// Validate the configuration before anything is spawned.
    fn validate(config: &EngineConfig) -> Result<()> {
        let engine_file = crate::services::ai::models::ENGINE_FILE_NAMES[0];
        if !config.engine_path.is_file() {
            return Err(AllInsightError::Ai(format!(
                "The local inference engine was not found. Point AllInsight at {engine_file} in Settings."
            )));
        }
        // llama.cpp ships the server under one of two names. Accepting only
        // those keeps a settings value from turning this into a way to start
        // any program on the machine.
        let engine_name = config
            .engine_path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !crate::services::ai::models::ENGINE_FILE_NAMES.contains(&engine_name.as_str()) {
            return Err(AllInsightError::Ai(format!(
                "The inference engine must be {engine_file} from a llama.cpp build."
            )));
        }
        #[cfg(windows)]
        if crate::services::security::paths::extension_lower(&config.engine_path) != "exe" {
            return Err(AllInsightError::Ai(
                "The inference engine must be an executable.".into(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let executable = std::fs::metadata(&config.engine_path)
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false);
            if !executable {
                return Err(AllInsightError::Ai(format!(
                    "{} is not marked as executable. Run chmod +x on it first.",
                    config.engine_path.display()
                )));
            }
        }
        if !config.model_path.is_file() {
            return Err(AllInsightError::Ai(
                "The selected model file no longer exists.".into(),
            ));
        }
        if crate::services::security::paths::extension_lower(&config.model_path) != "gguf" {
            return Err(AllInsightError::Ai("The model must be a GGUF file.".into()));
        }
        Ok(())
    }

    /// Start the engine. Blocking, and safe to call when it is already up.
    pub fn start(&self, config: EngineConfig) -> Result<EngineStatus> {
        Self::validate(&config)?;

        {
            let mut inner = self.inner.lock();
            if inner.status.state == EngineState::Ready {
                if inner
                    .config
                    .as_ref()
                    .map(|c| c.model_path == config.model_path)
                    .unwrap_or(false)
                {
                    return Ok(inner.status.clone());
                }
            }
            // A different model was requested: stop the old one first.
            Self::stop_locked(&mut inner);
            inner.status = EngineStatus {
                state: EngineState::Starting,
                model_name: config
                    .model_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned()),
                engine_path: Some(config.engine_path.clone()),
                port: None,
                message: "Loading the local model...".into(),
                offline_only: true,
            };
        }

        let port = pick_port();
        let api_key = new_api_key();
        let threads = if config.threads == 0 {
            // Leave a core for the interface.
            (num_cpus::get().saturating_sub(1)).max(1) as u32
        } else {
            config.threads
        };

        // Arguments are passed as a vector, never assembled into a command
        // line and never handed to a shell.
        let mut command = Command::new(&config.engine_path);
        command
            .arg("--model")
            .arg(&config.model_path)
            .arg("--ctx-size")
            .arg(config.context_size.to_string())
            .arg("--threads")
            .arg(threads.to_string())
            .arg("--n-gpu-layers")
            .arg(config.gpu_layers.to_string())
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            // Loopback still means every process on this machine can reach the
            // port. A key generated per launch means only AllInsight can use it.
            .arg("--api-key")
            .arg(&api_key)
            // No web interface: this server exists only for AllInsight.
            .arg("--no-webui")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: the engine must not flash a console.
            command.creation_flags(0x0800_0000);
        }

        let mut child = command.spawn().map_err(|e| {
            AllInsightError::Ai(format!("The local inference engine could not start: {e}"))
        })?;

        // Drain the engine's output so its pipes never fill and block it.
        // The lines go to the log, not to the user interface.
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
                    tracing::debug!(target: "allinsight::ai", "{line}");
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(std::result::Result::ok) {
                    tracing::debug!(target: "allinsight::ai", "{line}");
                }
            });
        }

        {
            let mut inner = self.inner.lock();
            inner.child = Some(child);
            inner.port = Some(port);
            inner.api_key = Some(api_key.clone());
            inner.config = Some(config.clone());
        }

        let alive = || {
            let mut inner = self.inner.lock();
            match inner.child.as_mut() {
                // `Ok(None)` is the engine still running; anything else means
                // it exited, or that its status can no longer be read.
                Some(child) => matches!(child.try_wait(), Ok(None)),
                None => false,
            }
        };

        match wait_until_ready(port, &api_key, alive) {
            Ok(()) => {
                let mut inner = self.inner.lock();
                inner.status.state = EngineState::Ready;
                inner.status.port = Some(port);
                inner.status.message = "The local model is loaded and ready.".into();
                Ok(inner.status.clone())
            }
            Err(e) => {
                let mut inner = self.inner.lock();
                Self::stop_locked(&mut inner);
                inner.status.state = EngineState::Failed;
                inner.status.message = e.to_string();
                Err(e)
            }
        }
    }

    pub fn stop(&self) {
        let mut inner = self.inner.lock();
        Self::stop_locked(&mut inner);
        inner.status = EngineStatus::default();
    }

    fn stop_locked(inner: &mut Inner) {
        if let Some(mut child) = inner.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        inner.port = None;
        inner.api_key = None;
    }

    /// Send a prompt and return the completion.
    pub fn complete(&self, system: &str, user: &str) -> Result<String> {
        let (port, api_key) = {
            let inner = self.inner.lock();
            if inner.status.state != EngineState::Ready {
                return Err(AllInsightError::Ai(
                    "The local model is not loaded.".into(),
                ));
            }
            let port = inner
                .port
                .ok_or_else(|| AllInsightError::Ai("The local model is not loaded.".into()))?;
            let key = inner
                .api_key
                .clone()
                .ok_or_else(|| AllInsightError::Ai("The local model is not loaded.".into()))?;
            (port, key)
        };

        let prompt = build_prompt(system, user);
        let body = serde_json::json!({
            "prompt": prompt,
            "n_predict": MAX_TOKENS,
            "temperature": 0.3,
            "top_p": 0.9,
            "repeat_penalty": 1.1,
            "stop": ["<|im_end|>", "<|eot_id|>", "</s>", "\nUser:", "\nSystem:"],
            "stream": false
        });

        let response = ureq::post(&format!("http://127.0.0.1:{port}/completion"))
            .timeout(Duration::from_secs(180))
            .set("Authorization", &format!("Bearer {api_key}"))
            .send_json(body)
            .map_err(|e| AllInsightError::Ai(format!("The local model did not respond: {e}")))?;

        let parsed: serde_json::Value = response
            .into_json()
            .map_err(|e| AllInsightError::Ai(format!("The local model returned an unreadable reply: {e}")))?;

        let content = parsed
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or_default();

        Ok(sanitise_output(content))
    }
}

impl Drop for LlamaEngine {
    fn drop(&mut self) {
        let mut inner = self.inner.lock();
        Self::stop_locked(&mut inner);
    }
}

/// ChatML framing, which every current instruction-tuned GGUF understands well
/// enough for this use. Kept in one place so swapping engines only changes
/// this function.
fn build_prompt(system: &str, user: &str) -> String {
    format!(
        "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"
    )
}

/// Clean model output before it reaches the interface.
///
/// The model's reply is data, never instructions and never a command. This
/// strips control characters and the invisible reordering marks that could be
/// used to make text render differently from what it says, and caps the
/// length so one reply cannot flood the view.
pub fn sanitise_output(raw: &str) -> String {
    const MAX_CHARS: usize = 4000;
    let cleaned: String = raw
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c,
                '\u{00AD}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}')
                || *c == '\n'
        })
        .collect();

    let trimmed = cleaned.trim();
    if trimmed.chars().count() > MAX_CHARS {
        let truncated: String = trimmed.chars().take(MAX_CHARS).collect();
        format!("{truncated}...")
    } else {
        trimmed.to_string()
    }
}

fn pick_port() -> u16 {
    for port in PORT_RANGE {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    PORT_RANGE.start
}

/// A fresh credential for one engine process.
///
/// The engine listens on loopback, which is not a boundary: every process
/// running as this user, and on a shared machine every other user's processes
/// too, can reach the port. The key means only AllInsight can send it work.
fn new_api_key() -> String {
    use std::hash::{BuildHasher, Hasher, RandomState};
    // Four draws from `RandomState`, which seeds itself from the operating
    // system once per thread and derives each later value from that seed. The
    // result is a 256-bit string carrying the seed's entropy, which is ample
    // for a token that lives as long as one engine process. No dependency is
    // added for it.
    let mut out = String::with_capacity(32);
    for _ in 0..4 {
        let value = RandomState::new().build_hasher().finish();
        out.push_str(&format!("{value:016x}"));
    }
    out
}

/// Poll the engine's health endpoint until it answers, or the engine dies.
///
/// `engine_alive` is checked first on every pass, and that ordering is the
/// point. `pick_port` proves a port is free by binding it and letting go, so
/// there is a window before the engine binds it in which any other process on
/// the machine can take it instead. If that happens the engine exits, and
/// without this check the loop would go on to hand the engine's credential to
/// whatever is now listening and accept its 200 as proof the model had loaded.
/// Every later answer would then come from that process rather than the model.
fn wait_until_ready(
    port: u16,
    api_key: &str,
    mut engine_alive: impl FnMut() -> bool,
) -> Result<()> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let url = format!("http://127.0.0.1:{port}/health");
    let mut last_error = String::new();

    while Instant::now() < deadline {
        if !engine_alive() {
            return Err(AllInsightError::Ai(
                "The local inference engine stopped before it finished loading. \
                 Another process may have taken the port it was given."
                    .into(),
            ));
        }

        match ureq::get(&url)
            .timeout(Duration::from_secs(2))
            .set("Authorization", &format!("Bearer {api_key}"))
            .call()
        {
            Ok(response) if response.status() == 200 => return Ok(()),
            Ok(response) => last_error = format!("engine reported status {}", response.status()),
            Err(e) => last_error = e.to_string(),
        }
        std::thread::sleep(Duration::from_millis(400));
    }

    Err(AllInsightError::Ai(format!(
        "The local model did not finish loading within {} seconds. {last_error}",
        STARTUP_TIMEOUT.as_secs()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_that_died_is_not_mistaken_for_one_that_is_loading() {
        // Port 1 has nothing on it, but the call must never get as far as
        // asking: a dead engine means the port may belong to someone else now,
        // and the credential must not be offered to them.
        let started = Instant::now();
        let result = wait_until_ready(1, "unused", || false);

        let error = result.expect_err("a dead engine must not report ready");
        assert!(error.to_string().contains("stopped before it finished loading"));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "it must fail at once rather than polling until the startup timeout"
        );
    }

    #[test]
    fn output_is_stripped_of_invisible_reordering_marks() {
        let hostile = "Delete C:\\Windows\u{202E} now\u{0007}";
        let clean = sanitise_output(hostile);
        assert!(!clean.contains('\u{202E}'));
        assert!(!clean.contains('\u{0007}'));
        assert!(clean.contains("Delete C:\\Windows"));
    }

    #[test]
    fn newlines_survive_sanitising() {
        let text = "First line.\nSecond line.";
        assert_eq!(sanitise_output(text), text);
    }

    #[test]
    fn very_long_output_is_capped() {
        let long = "a".repeat(10_000);
        let clean = sanitise_output(&long);
        assert!(clean.chars().count() <= 4003);
        assert!(clean.ends_with("..."));
    }

    #[test]
    fn the_prompt_keeps_the_system_and_user_parts_separate() {
        let prompt = build_prompt("Rules here.", "Question here.");
        assert!(prompt.contains("<|im_start|>system\nRules here.<|im_end|>"));
        assert!(prompt.contains("<|im_start|>user\nQuestion here.<|im_end|>"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn an_engine_that_does_not_exist_is_refused_before_spawning() {
        let engine = LlamaEngine::new();
        let err = engine
            .start(EngineConfig {
                engine_path: PathBuf::from("Z:\\nope\\llama-server.exe"),
                model_path: PathBuf::from("Z:\\nope\\model.gguf"),
                context_size: 2048,
                threads: 2,
                gpu_layers: 0,
            })
            .unwrap_err();
        assert!(err.to_string().contains("was not found"));
        assert_eq!(engine.status().state, EngineState::Idle);
    }

    #[test]
    fn completing_without_a_loaded_model_is_a_clear_error() {
        let engine = LlamaEngine::new();
        let err = engine.complete("system", "user").unwrap_err();
        assert!(err.to_string().contains("not loaded"));
    }

    #[test]
    fn the_default_status_says_the_app_works_without_a_model() {
        let status = EngineStatus::default();
        assert!(status.offline_only);
        assert!(status.message.contains("without one"));
    }
}
