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

/// Combined output from a completed two-step agent turn.
pub struct AgentOutput {
    /// Internal analysis from the reasoning model — displayed to the user
    /// but not stored in conversation history.
    pub reasoning: String,
    /// Final reply from the decision model — displayed and stored in history.
    pub answer: String,
}

/// Drives a single user turn through the two-step reasoning→decision pipeline.
///
/// `run_reasoning` and `run_decision` are exposed as separate public steps so
/// the CLI can display each phase (with its own spinner) as it completes.
/// `run` is a convenience wrapper that executes both steps in sequence.
pub struct Agent<'a> {
    client: &'a ApiClient,
}

impl<'a> Agent<'a> {
    pub fn new(client: &'a ApiClient) -> Self {
        Self { client }
    }

    /// Step 1 — send the full history to the reasoning model.
    /// Returns the internal analysis string; does not modify history.
    pub fn run_reasoning(&self, history: &[ChatMessage]) -> anyhow::Result<String> {
        let reasoning_history = Self::replace_system(history, REASONING_SYSTEM);
        self.client.reason(&reasoning_history, Some(512))
    }

    /// Step 2 — send a compact context (system + analysis + last user message)
    /// to the decision model. Returns the final answer string.
    pub fn run_decision(&self, history: &[ChatMessage], analysis: &str) -> anyhow::Result<String> {
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

        self.client.decide(&decision_messages, Some(256))
    }

    /// Convenience: run both steps and return combined output.
    pub fn run(&self, history: &[ChatMessage]) -> anyhow::Result<AgentOutput> {
        let reasoning = self.run_reasoning(history)?;
        let answer = self.run_decision(history, &reasoning)?;
        Ok(AgentOutput { reasoning, answer })
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
