use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{error, info, instrument, warn};
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    routing::post,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::model::ModelRunner;

// ── OpenAI-compatible request/response types ─────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    /// "system" | "user" | "assistant"
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    /// Overrides the model-config default when provided.
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct ChatUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: ChatUsage,
}

// ── Prompt builder ────────────────────────────────────────────────────────────

/// Converts an OpenAI `messages` array into a Mistral/Llama `[INST]` prompt.
///
/// - A leading `system` message is prepended to the first user turn.
/// - `user` / `assistant` pairs build multi-turn history.
/// - The final user message is left open (no trailing assistant token) so the
///   model continues from there.
pub fn build_prompt(messages: &[ChatMessage]) -> String {
    let mut result = String::new();

    // Split off an optional leading system message.
    let (system_opt, turns) = match messages.first() {
        Some(m) if m.role == "system" => (Some(m.content.as_str()), &messages[1..]),
        _ => (None, messages),
    };

    let mut iter = turns.iter();
    let mut first_user = true;

    while let Some(msg) = iter.next() {
        match msg.role.as_str() {
            "user" => {
                if first_user {
                    match system_opt {
                        Some(sys) => result.push_str(&format!(
                            "<s>[INST] {}\n\n{} [/INST]",
                            sys, msg.content
                        )),
                        None => result
                            .push_str(&format!("<s>[INST] {} [/INST]", msg.content)),
                    }
                    first_user = false;
                } else {
                    result.push_str(&format!("[INST] {} [/INST]", msg.content));
                }
            }
            "assistant" => {
                // Closed assistant turn — part of history, not the live response.
                result.push_str(&format!(" {} </s>", msg.content));
            }
            _ => {} // ignore unknown roles
        }
    }

    result
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn create_router(runner: Arc<ModelRunner>) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(runner)
}

// ── Handler ───────────────────────────────────────────────────────────────────

#[instrument(skip(runner, req), fields(model, message_count = req.messages.len(), max_tokens = ?req.max_tokens))]
async fn chat_completions(
    State(runner): State<Arc<ModelRunner>>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, String)> {
    if req.messages.is_empty() {
        warn!("rejected request: messages array is empty");
        return Err((StatusCode::BAD_REQUEST, "messages must not be empty".into()));
    }

    let model_name = req.model.clone().unwrap_or_else(|| "LLML".to_string());
    let max_tokens = req.max_tokens;
    let message_count = req.messages.len();

    info!(
        model = %model_name,
        message_count,
        max_tokens = ?max_tokens,
        "received chat completion request"
    );

    let prompt = build_prompt(&req.messages);
    info!(prompt_len = prompt.len(), "prompt built");

    // llama_cpp inference is blocking; run it off the async executor.
    let output = tokio::task::spawn_blocking(move || {
        runner.generate_from_prompt(&prompt, max_tokens)
    })
    .await
    .map_err(|e| {
        error!(error = %e, "spawn_blocking join error");
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?
    .map_err(|e| {
        error!(error = %e, "inference error");
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    info!(output_len = output.len(), "inference returned, sending response");

    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(Json(ChatResponse {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created,
        model: model_name,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: output,
            },
            finish_reason: "stop".to_string(),
        }],
        usage: ChatUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        },
    }))
}
