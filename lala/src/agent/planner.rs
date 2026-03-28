use crate::agent::model::{ApiClient, ChatMessage};

/// System prompt injected for the reasoning step.
///
/// The reasoning model's job is to silently think through the query —
/// it never speaks directly to the user.
const REASONING_SYSTEM: &str =
    "You are an internal reasoning engine. \
     Analyse the user's query carefully. \
     Think step by step: what is the user asking, what context matters, \
     and what would make the best answer. \
     Output your analysis concisely — this will be used to guide the final response, \
     not shown to the user.";

/// System prompt for the decision step.
///
/// The decision model receives the original conversation history plus the
/// reasoning output as extra context, and produces the reply the user sees.
const DECISION_SYSTEM: &str =
    "You are lala, a friendly and concise AI assistant. \
     You have been given an internal analysis to guide you. \
     Use it to inform your answer but do NOT repeat or quote it. \
     Respond directly to the user in clear, natural language.";

/// Drives a single user turn through the two-step reasoning→decision pipeline.
///
/// # Steps
///
/// 1. **Reason** — sends the full conversation history to the `reasoning` model
///    with a reasoning-specific system prompt. The output is an internal analysis
///    that is never shown to the user.
///
/// 2. **Decide** — sends a condensed prompt to the `decision` model containing:
///    - the conversation history (system prompt replaced with DECISION_SYSTEM)
///    - the reasoning output injected as a hidden `system` message
///
///    The decision model produces the final reply shown to the user.
pub struct Agent<'a> {
    client: &'a ApiClient,
}

impl<'a> Agent<'a> {
    pub fn new(client: &'a ApiClient) -> Self {
        Self { client }
    }

    pub fn run(&self, history: &[ChatMessage]) -> anyhow::Result<String> {
        // ── Step 1: Reason ────────────────────────────────────────────────
        // Replace the CLI system prompt with the reasoning-specific one so the
        // model understands its role for this step.
        let reasoning_history = Self::replace_system(history, REASONING_SYSTEM);
        let analysis = self.client.reason(&reasoning_history, Some(512))?;

        // ── Step 2: Decide ────────────────────────────────────────────────
        // Use the decision model's system prompt and inject the reasoning
        // output as an additional hidden context message before the final
        // user turn. The decision model has a small context window (512 tokens)
        // so we only include the last user message, not the full history.
        let last_user = history
            .iter()
            .rfind(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let decision_messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: DECISION_SYSTEM.to_string(),
            },
            ChatMessage {
                role: "system".to_string(),
                content: format!("[Internal analysis — do not quote this]\n{}", analysis),
            },
            ChatMessage {
                role: "user".to_string(),
                content: last_user.to_string(),
            },
        ];

        let answer = self.client.decide(&decision_messages, Some(256))?;
        Ok(answer)
    }

    /// Returns a copy of `history` with the first `system` message replaced
    /// by `new_system`. If no system message is present, prepends it.
    fn replace_system(history: &[ChatMessage], new_system: &str) -> Vec<ChatMessage> {
        let mut out = history.to_vec();
        let new_msg = ChatMessage {
            role: "system".to_string(),
            content: new_system.to_string(),
        };
        if out.first().map(|m| m.role.as_str()) == Some("system") {
            out[0] = new_msg;
        } else {
            out.insert(0, new_msg);
        }
        out
    }
}
