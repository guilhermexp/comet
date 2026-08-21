//! Native Oh My Pi driver over `omp --mode rpc-ui`.

use std::collections::HashSet;

use serde_json::{Value, json};
use zeron_proto::{Model, ReasoningLevel, SlashCommand};

use crate::HarnessError;

#[doc(hidden)]
pub mod normalize;
#[doc(hidden)]
pub mod process;
#[doc(hidden)]
pub mod protocol;

const OMP_REASONING_LEVELS: &[ReasoningLevel] = &[
    ReasoningLevel::Minimal,
    ReasoningLevel::Low,
    ReasoningLevel::Medium,
    ReasoningLevel::High,
    ReasoningLevel::XHigh,
    ReasoningLevel::Max,
];

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
