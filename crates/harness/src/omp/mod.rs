//! Native Oh My Pi driver over `omp --mode rpc-ui`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use futures::{StreamExt as _, stream::BoxStream};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SlashCommand,
    SteeringMode, UserInputAnswer, UserInputQuestion,
};

use self::normalize::{AgentEndDisposition, OmpNormalizer};
use self::process::{OmpLaunch, OmpProcess};
use self::workers_bridge::{WorkersBridge, WorkersBridgeOptions};
use crate::{Harness, HarnessError, RunControls, SteerMessage};

#[doc(hidden)]
pub mod normalize;
#[doc(hidden)]
pub mod process;
#[doc(hidden)]
pub mod protocol;
#[doc(hidden)]
pub mod workers_bridge;

const OMP_REASONING_LEVELS: &[ReasoningLevel] = &[
    ReasoningLevel::Minimal,
    ReasoningLevel::Low,
    ReasoningLevel::Medium,
    ReasoningLevel::High,
    ReasoningLevel::XHigh,
    ReasoningLevel::Max,
];

pub struct OmpHarness {
    executable: Option<PathBuf>,
    workers_mcp_executable: Option<PathBuf>,
    env: Option<HashMap<String, String>>,
    handshake_timeout: Duration,
    request_timeout: Duration,
}

impl Default for OmpHarness {
    fn default() -> Self {
        Self {
            executable: None,
            workers_mcp_executable: None,
            env: None,
            handshake_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(10),
        }
    }
}

impl OmpHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_executable(mut self, executable: impl Into<PathBuf>) -> Self {
        self.executable = Some(executable.into());
        self
    }

    pub fn with_workers_mcp_executable(mut self, executable: impl Into<PathBuf>) -> Self {
        self.workers_mcp_executable = Some(executable.into());
        self
    }

    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = Some(env);
        self
    }

    pub fn with_timeouts(mut self, handshake: Duration, request: Duration) -> Self {
        self.handshake_timeout = handshake;
        self.request_timeout = request;
        self
    }

    fn resolve_executable(&self) -> Option<PathBuf> {
        self.executable
            .clone()
            .or_else(|| {
                self.env
                    .as_ref()
                    .and_then(|env| env.get("OMP_EXECUTABLE"))
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
            })
            .or_else(|| {
                std::env::var_os("OMP_EXECUTABLE")
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
            })
            .or_else(|| crate::acp::find_on_paths("omp", Vec::new()))
    }

    fn launch(&self, cwd: PathBuf, ephemeral: bool) -> Result<OmpLaunch, HarnessError> {
        Ok(OmpLaunch {
            executable: self.resolve_executable().ok_or_else(|| {
                HarnessError::NotInstalled(
                    "omp (searched PATH, the login shell's PATH, and known Bun/npm install dirs; set OMP_EXECUTABLE to override)".into(),
                )
            })?,
            cwd,
            ephemeral,
            env: self.env.clone(),
            handshake_timeout: self.handshake_timeout,
            request_timeout: self.request_timeout,
        })
    }

    fn workers_executable(&self) -> Option<PathBuf> {
        self.workers_mcp_executable
            .clone()
            .or_else(|| {
                self.env
                    .as_ref()
                    .and_then(|env| env.get("ZERON_WORKERS_MCP_BIN"))
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
            })
            .or_else(|| std::env::var_os("ZERON_WORKERS_MCP_BIN").map(PathBuf::from))
            .or_else(|| std::env::current_exe().ok())
    }

    fn workers_disabled(&self) -> bool {
        self.env
            .as_ref()
            .and_then(|env| env.get("ZERON_DISABLE_WORKERS_MCP"))
            .is_some_and(|value| value == "1")
            || std::env::var("ZERON_DISABLE_WORKERS_MCP")
                .ok()
                .is_some_and(|value| value == "1")
    }
}

#[async_trait]
impl Harness for OmpHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Omp
    }

    fn display_name(&self) -> &str {
        "OMP"
    }

    fn supports_steering(&self) -> bool {
        true
    }

    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::StepBoundary
    }

    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        OMP_REASONING_LEVELS
    }

    fn installed(&self) -> bool {
        self.resolve_executable().is_some()
    }

    fn deterministic_turn_end(&self) -> bool {
        true
    }

    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        discover_models_with_launch(self.launch(std::env::current_dir()?, true)?).await
    }

    async fn commands(&self) -> Result<Vec<SlashCommand>, HarnessError> {
        discover_commands_with_launch(self.launch(std::env::current_dir()?, true)?).await
    }

    async fn run(
        &self,
        request: RunRequest,
        controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        let process = OmpProcess::start(self.launch(PathBuf::from(&request.cwd), false)?).await?;
        let events = process.take_events()?;
        process
            .request(json!({ "type": "set_subagent_subscription", "level": "events" }))
            .await?;
        if let Some(session_path) = request.resume.as_deref() {
            let response = process
                .request(json!({ "type": "switch_session", "sessionPath": session_path }))
                .await?;
            if response.get("cancelled").and_then(Value::as_bool) == Some(true) {
                return Err(HarnessError::Protocol(
                    "OMP session resume was cancelled".into(),
                ));
            }
        }

        let workers = if request.enable_workers_mcp && !self.workers_disabled() {
            let executable = self.workers_executable().ok_or_else(|| {
                HarnessError::Protocol("Workers controller executable is unavailable".into())
            })?;
            WorkersBridge::start(WorkersBridgeOptions {
                enabled: true,
                executable,
                parent_chat_id: request.workers_parent_chat_id.clone(),
            })
            .await?
            .map(Arc::new)
        } else {
            None
        };
        if let Some(workers) = &workers {
            process
                .request(json!({
                    "type": "set_host_tools",
                    "tools": [workers.definition().clone()]
                }))
                .await?;
        }

        if let Some(model) = request.model.as_deref() {
            let (provider, model_id) = split_model_id(model)?;
            process
                .request(json!({
                    "type": "set_model",
                    "provider": provider,
                    "modelId": model_id
                }))
                .await?;
        }
        if let Some(reasoning) = request.reasoning {
            process
                .request(json!({
                    "type": "set_thinking_level",
                    "level": reasoning_wire(reasoning)
                }))
                .await?;
        }

        let state = process.request(json!({ "type": "get_state" })).await?;
        let model = request
            .model
            .clone()
            .or_else(|| state_model(&state))
            .unwrap_or_default();
        let session_id = state_session_id(&state).unwrap_or_default();
        let tools = state
            .get("dumpTools")
            .and_then(Value::as_array)
            .map(|tools| {
                tools
                    .iter()
                    .filter_map(|tool| tool.get("name").and_then(Value::as_str))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let images = load_images(&request.attachments).await;
        let mut prompt = json!({ "type": "prompt", "message": request.prompt });
        if !images.is_empty() {
            prompt["images"] = Value::Array(images);
        }
        let prompt_response = process.request(prompt).await?;

        let (event_tx, event_rx) = mpsc::channel::<Result<AgentEvent, HarnessError>>(256);
        event_tx
            .send(Ok(AgentEvent::SessionStarted {
                harness: HarnessId::Omp,
                model: model.clone(),
                tools,
                cwd: request.cwd.clone(),
                session_id: session_id.clone(),
                assistant_message_id: uuid::Uuid::new_v4().to_string(),
            }))
            .await
            .map_err(|_| HarnessError::Protocol("OMP event consumer closed".into()))?;
        if prompt_response.get("agentInvoked").and_then(Value::as_bool) == Some(false) {
            event_tx
                .send(Ok(AgentEvent::Done {
                    status: DoneStatus::Completed,
                    result: None,
                    error: None,
                    session_id: Some(session_id),
                }))
                .await
                .ok();
            if let Some(workers) = workers {
                let _ = workers.shutdown().await;
            }
            let _ = process.shutdown().await;
        } else {
            tokio::spawn(run_session(
                process,
                events,
                event_tx,
                controls,
                workers,
                request.cwd,
                model,
                session_id,
            ));
        }

        Ok(
            futures::stream::unfold(event_rx, |mut receiver| async move {
                receiver.recv().await.map(|event| (event, receiver))
            })
            .boxed(),
        )
    }
}

#[doc(hidden)]
pub async fn discover_models_with_launch(
    mut launch: process::OmpLaunch,
) -> Result<Vec<Model>, HarnessError> {
    launch.ephemeral = true;
    let process = process::OmpProcess::start(launch).await?;
    let (state, available) = tokio::join!(
        process.request(json!({ "type": "get_state" })),
        process.request(json!({ "type": "get_available_models" })),
    );
    let shutdown = process.shutdown().await;
    let state = state?;
    let available = available?;
    shutdown?;
    map_models(&state, &available)
}

#[doc(hidden)]
pub async fn discover_commands_with_launch(
    mut launch: process::OmpLaunch,
) -> Result<Vec<SlashCommand>, HarnessError> {
    launch.ephemeral = true;
    let process = process::OmpProcess::start(launch).await?;
    let result = process
        .request(json!({ "type": "get_available_commands" }))
        .await;
    let shutdown = process.shutdown().await;
    let result = result?;
    shutdown?;
    map_commands(&result)
}

fn map_models(state: &Value, response: &Value) -> Result<Vec<Model>, HarnessError> {
    let rows = response
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| HarnessError::Protocol("OMP model response omitted models".into()))?;
    if rows.len() > 1_000 {
        return Err(HarnessError::Protocol(
            "OMP model response exceeded 1000 rows".into(),
        ));
    }
    let mut seen = HashSet::new();
    let mut models = Vec::with_capacity(rows.len());
    for row in rows {
        let provider = bounded_string(row, "provider", 160)?;
        let id = bounded_string(row, "id", 240)?;
        let name = row
            .get("name")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty() && value.len() <= 240)
            .unwrap_or(id);
        let composite = compose_model_id(provider, id);
        if !seen.insert(composite.clone()) {
            return Err(HarnessError::Protocol(format!(
                "OMP advertised duplicate model {composite}"
            )));
        }
        models.push(Model {
            id: composite,
            label: format!("{provider}/{name}"),
            description: None,
            reasoning_levels: if row.get("reasoning").and_then(Value::as_bool) == Some(true) {
                OMP_REASONING_LEVELS.to_vec()
            } else {
                Vec::new()
            },
            options: Vec::new(),
        });
    }
    let current = state
        .get("model")
        .and_then(Value::as_object)
        .and_then(|model| {
            let provider = model.get("provider")?.as_str()?;
            let id = model.get("id")?.as_str()?;
            Some(compose_model_id(provider, id))
        });
    if let Some(current) = current
        && let Some(index) = models.iter().position(|model| model.id == current)
    {
        models.rotate_left(index);
    }
    Ok(models)
}

fn map_commands(response: &Value) -> Result<Vec<SlashCommand>, HarnessError> {
    let rows = response
        .get("commands")
        .and_then(Value::as_array)
        .ok_or_else(|| HarnessError::Protocol("OMP command response omitted commands".into()))?;
    if rows.len() > 1_000 {
        return Err(HarnessError::Protocol(
            "OMP command response exceeded 1000 rows".into(),
        ));
    }
    let mut seen = HashSet::new();
    let mut commands = Vec::new();
    for row in rows {
        let name = bounded_string(row, "name", 160)?;
        if !seen.insert(name.to_owned()) {
            continue;
        }
        let description = row
            .get("description")
            .and_then(Value::as_str)
            .filter(|value| value.len() <= 1_024)
            .unwrap_or_default()
            .to_owned();
        let input_hint = row
            .pointer("/input/hint")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty() && value.len() <= 240)
            .map(str::to_owned);
        commands.push(SlashCommand {
            name: name.to_owned(),
            description,
            input_hint,
        });
    }
    Ok(commands)
}

pub(crate) fn compose_model_id(provider: &str, model: &str) -> String {
    format!("{provider}/{model}")
}

fn bounded_string<'a>(row: &'a Value, key: &str, max: usize) -> Result<&'a str, HarnessError> {
    row.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= max)
        .ok_or_else(|| HarnessError::Protocol(format!("OMP {key} is missing or invalid")))
}

fn split_model_id(value: &str) -> Result<(&str, &str), HarnessError> {
    let (provider, model) = value
        .split_once('/')
        .ok_or_else(|| HarnessError::Protocol("OMP model id must be <provider>/<model>".into()))?;
    if provider.trim().is_empty() || model.trim().is_empty() {
        return Err(HarnessError::Protocol(
            "OMP model id must contain a provider and model".into(),
        ));
    }
    Ok((provider, model))
}

fn reasoning_wire(reasoning: ReasoningLevel) -> &'static str {
    match reasoning {
        ReasoningLevel::Minimal => "minimal",
        ReasoningLevel::Low => "low",
        ReasoningLevel::Medium => "medium",
        ReasoningLevel::High => "high",
        ReasoningLevel::XHigh | ReasoningLevel::Ultracode => "xhigh",
        ReasoningLevel::Max | ReasoningLevel::Ultra | ReasoningLevel::Ultrathink => "max",
    }
}

fn state_model(state: &Value) -> Option<String> {
    let model = state.get("model")?;
    Some(compose_model_id(
        model.get("provider")?.as_str()?,
        model.get("id")?.as_str()?,
    ))
}

fn state_session_id(state: &Value) -> Option<String> {
    state
        .get("sessionFile")
        .and_then(Value::as_str)
        .or_else(|| state.get("sessionId").and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[allow(clippy::too_many_arguments)]
async fn run_session(
    process: OmpProcess,
    mut events: mpsc::Receiver<Value>,
    event_tx: mpsc::Sender<Result<AgentEvent, HarnessError>>,
    controls: RunControls,
    workers: Option<Arc<WorkersBridge>>,
    cwd: String,
    model: String,
    mut session_id: String,
) {
    let RunControls {
        request_input,
        mut steering,
        interrupt,
    } = controls;
    let request_input: Arc<
        dyn Fn(Vec<UserInputQuestion>) -> tokio::sync::oneshot::Receiver<Vec<UserInputAnswer>>
            + Send
            + Sync,
    > = request_input.into();
    let mut normalizer = OmpNormalizer::new(cwd, model);
    let mut pending_agent_end: Option<Value> = None;
    let mut steering_open = true;
    let mut finished = false;

    while !finished {
        tokio::select! {
            _ = interrupt.cancelled() => {
                let _ = tokio::time::timeout(
                    Duration::from_millis(500),
                    process.request(json!({ "type": "abort" })),
                ).await;
                let _ = emit(&event_tx, AgentEvent::Done {
                    status: DoneStatus::Interrupted,
                    result: None,
                    error: None,
                    session_id: Some(session_id.clone()),
                }).await;
                finished = true;
            }
            steer = steering.recv(), if steering_open => {
                match steer {
                    Some(SteerMessage { prompt, message_id }) => {
                        let steer_process = process.clone();
                        let steer_events = event_tx.clone();
                        tokio::spawn(async move {
                            match steer_process.request(json!({ "type": "steer", "message": prompt })).await {
                                Ok(_) => {
                                    let _ = emit(&steer_events, AgentEvent::Steered {
                                        assistant_message_id: message_id,
                                        next_assistant_message_id: Some(uuid::Uuid::new_v4().to_string()),
                                    }).await;
                                }
                                Err(error) => {
                                    let _ = emit(&steer_events, AgentEvent::Error { message: error.to_string() }).await;
                                }
                            }
                        });
                    }
                    None => steering_open = false,
                }
            }
            frame = events.recv() => {
                let Some(frame) = frame else {
                    let message = "OMP RPC stdout closed before terminal agent_end".to_owned();
                    let _ = emit(&event_tx, AgentEvent::Error { message: message.clone() }).await;
                    let _ = emit(&event_tx, AgentEvent::Done {
                        status: DoneStatus::Errored,
                        result: None,
                        error: Some(message),
                        session_id: Some(session_id.clone()),
                    }).await;
                    break;
                };
                match frame.get("type").and_then(Value::as_str) {
                    Some("host_tool_call") => {
                        let id = frame.get("id").and_then(Value::as_str).unwrap_or_default();
                        let tool = frame.get("toolName").and_then(Value::as_str).unwrap_or_default();
                        let arguments = frame.get("arguments").cloned().unwrap_or(Value::Null);
                        let result = match &workers {
                            Some(workers) => workers.handle_call(id, tool, arguments).await,
                            None => json!({
                                "type": "host_tool_result",
                                "id": id,
                                "result": { "content": [{ "type": "text", "text": "Workers are disabled for this run" }] },
                                "isError": true
                            }),
                        };
                        if let Err(error) = process.send_control(result) {
                            let _ = emit(&event_tx, AgentEvent::Error { message: error.to_string() }).await;
                        }
                    }
                    Some("extension_ui_request") => {
                        if answer_interactive_request(&process, &request_input, &interrupt, &frame).await {
                            finished = true;
                        }
                    }
                    Some("agent_end") => {
                        if frame.get("isTerminal").and_then(Value::as_bool) == Some(false) {
                            continue;
                        }
                        match normalizer.classify_agent_end(&frame) {
                            AgentEndDisposition::Continue => pending_agent_end = Some(frame),
                            disposition => {
                                finished = finish_agent_end(
                                    &process,
                                    &event_tx,
                                    &mut session_id,
                                    disposition,
                                ).await;
                            }
                        }
                    }
                    _ => {
                        for event in normalizer.push(frame) {
                            if !emit(&event_tx, event).await {
                                finished = true;
                                break;
                            }
                        }
                        if !finished
                            && normalizer.active_subagents() == 0
                            && let Some(frame) = pending_agent_end.take()
                        {
                            let disposition = normalizer.classify_agent_end(&frame);
                            finished = finish_agent_end(
                                &process,
                                &event_tx,
                                &mut session_id,
                                disposition,
                            ).await;
                        }
                    }
                }
            }
        }
    }

    if let Some(workers) = workers {
        let _ = workers.shutdown().await;
    }
    let _ = process.shutdown().await;
}

async fn finish_agent_end(
    process: &OmpProcess,
    event_tx: &mpsc::Sender<Result<AgentEvent, HarnessError>>,
    session_id: &mut String,
    disposition: AgentEndDisposition,
) -> bool {
    if disposition == AgentEndDisposition::Continue {
        return false;
    }
    if let Ok(state) = process.request(json!({ "type": "get_state" })).await
        && let Some(current) = state_session_id(&state)
    {
        *session_id = current;
    }
    match disposition {
        AgentEndDisposition::Complete => {
            let _ = emit(
                event_tx,
                AgentEvent::Done {
                    status: DoneStatus::Completed,
                    result: None,
                    error: None,
                    session_id: Some(session_id.clone()),
                },
            )
            .await;
            true
        }
        AgentEndDisposition::Error(message) => {
            if !emit(
                event_tx,
                AgentEvent::Error {
                    message: message.clone(),
                },
            )
            .await
            {
                return true;
            }
            let _ = emit(
                event_tx,
                AgentEvent::Done {
                    status: DoneStatus::Errored,
                    result: None,
                    error: Some(message),
                    session_id: Some(session_id.clone()),
                },
            )
            .await;
            true
        }
        AgentEndDisposition::Continue => false,
    }
}

async fn emit(
    event_tx: &mpsc::Sender<Result<AgentEvent, HarnessError>>,
    event: AgentEvent,
) -> bool {
    event_tx.send(Ok(event)).await.is_ok()
}

async fn answer_interactive_request(
    process: &OmpProcess,
    request_input: &Arc<
        dyn Fn(Vec<UserInputQuestion>) -> tokio::sync::oneshot::Receiver<Vec<UserInputAnswer>>
            + Send
            + Sync,
    >,
    interrupt: &tokio_util::sync::CancellationToken,
    frame: &Value,
) -> bool {
    let Some(id) = frame.get("id").and_then(Value::as_str) else {
        return false;
    };
    let method = frame
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let title = frame
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("OMP question");
    let question_text = frame
        .get("message")
        .or_else(|| frame.get("placeholder"))
        .or_else(|| frame.get("prefill"))
        .and_then(Value::as_str)
        .unwrap_or(title);
    let options = match method {
        "select" => frame
            .get("options")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        "confirm" => vec!["Yes".into(), "No".into()],
        "input" | "editor" => Vec::new(),
        _ => {
            let _ = process.send_control(json!({
                "type": "extension_ui_response",
                "id": id,
                "cancelled": true
            }));
            return false;
        }
    };
    let question = UserInputQuestion {
        id: id.to_owned(),
        header: truncate_text(title, 80),
        question: truncate_text(question_text, 4_096),
        options,
        multi_select: false,
    };
    let answers = tokio::select! {
        answers = (request_input)(vec![question]) => answers.ok(),
        _ = interrupt.cancelled() => None,
    };
    let label = answers
        .as_ref()
        .and_then(|answers| answers.first())
        .and_then(|answer| answer.labels.first());
    let response = match (method, label) {
        ("confirm", Some(label)) => json!({
            "type": "extension_ui_response",
            "id": id,
            "confirmed": label.eq_ignore_ascii_case("yes")
        }),
        ("select" | "input" | "editor", Some(label)) => json!({
            "type": "extension_ui_response",
            "id": id,
            "value": label
        }),
        _ => json!({
            "type": "extension_ui_response",
            "id": id,
            "cancelled": true
        }),
    };
    let _ = process.send_control(response);
    interrupt.is_cancelled()
}

async fn load_images(paths: &[String]) -> Vec<Value> {
    const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
    let mut images = Vec::new();
    for path in paths {
        let Ok(metadata) = tokio::fs::metadata(path).await else {
            continue;
        };
        if metadata.len() > MAX_IMAGE_BYTES {
            continue;
        }
        let Ok(bytes) = tokio::fs::read(path).await else {
            continue;
        };
        let Some(mime_type) = image_mime_type(path, &bytes) else {
            continue;
        };
        images.push(json!({
            "type": "image",
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
            "mimeType": mime_type
        }));
    }
    images
}

fn image_mime_type(path: &str, bytes: &[u8]) -> Option<&'static str> {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        _ => match bytes {
            [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
            [0xff, 0xd8, 0xff, ..] => Some("image/jpeg"),
            [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
            [
                b'R',
                b'I',
                b'F',
                b'F',
                _,
                _,
                _,
                _,
                b'W',
                b'E',
                b'B',
                b'P',
                ..,
            ] => Some("image/webp"),
            _ => None,
        },
    }
}

fn truncate_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
