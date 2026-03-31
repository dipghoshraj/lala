use crate::agent::model::{ApiClient, ChatMessage, RouteDecision};
use crate::agent::planner::{Agent, needs_reasoning};
use rag::RagStore;

use super::display;

const SYSTEM_PROMPT: &str =
    "You are a friendly AI assistant named lala. \
     Explain things clearly and naturally. \
     Respond in full sentences.";

/// Owns conversation history and drives the chat pipeline.
pub struct Chat<'a> {
    agent: Agent<'a>,
    smart_router: bool,
    history: Vec<ChatMessage>,
}

impl<'a> Chat<'a> {
    pub fn new(client: &'a ApiClient, smart_router: bool, store: &'a RagStore) -> Self {
        let history = vec![ChatMessage {
            role: "system".to_string(),
            content: SYSTEM_PROMPT.to_string(),
        }];

        Self {
            agent: Agent::new(client, store),
            smart_router,
            history,
        }
    }

    /// Clear conversation history, keeping only the system prompt.
    pub fn clear(&mut self) {
        self.history.truncate(1);
        display::success("Conversation cleared.");
        println!();
    }

    /// Process a user message through the routing → inference pipeline.
    pub fn handle(&mut self, input: &str) {
        self.history.push(ChatMessage {
            role: "user".to_string(),
            content: input.to_string(),
        });

        let route = self.classify(input);

        match route {
            RouteDecision::Direct => self.run_direct(),
            RouteDecision::Reasoning => self.run_reasoning(),
        }
    }

    // ── Internal ──────────────────────────────────────────────────────────

    fn classify(&self, input: &str) -> RouteDecision {
        if self.smart_router {
            self.agent.classify_query(input, &self.history)
        } else if needs_reasoning(input) {
            RouteDecision::Reasoning
        } else {
            RouteDecision::Direct
        }
    }

    fn run_direct(&mut self) {
        let result = display::with_spinner("thinking", || {
            self.agent.run_direct(&self.history)
        });

        match result {
            Ok(reply) => {
                display::print_section("Answer", display::BOLD_CYAN, display::CYAN, &reply);
                self.history.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: reply,
                });
            }
            Err(e) => {
                display::error(&format!("Error: {e}"));
                self.history.pop();
            }
        }
    }

    fn run_reasoning(&mut self) {
        // Retrieve context from RAG store.
        let input = match self.history.iter().rfind(|m| m.role == "user") {
            Some(m) => m.content.clone(),
            None => {
                display::error("No user message found.");
                return;
            }
        };

        let chunks = match display::with_spinner("retrieving", || {
            self.agent.retrieve_context(&input)
        }) {
            Ok(c) => c,
            Err(e) => {
                display::warn(&format!("Retrieval error: {e} — proceeding without context."));
                Vec::new()
            }
        };

        // Display retrieved sources if any were found.
        if !chunks.is_empty() {
            display::print_sources(&chunks);
        }

        // Build context string for LLM injection.
        let context_str = if chunks.is_empty() {
            None
        } else {
            Some(
                chunks
                    .iter()
                    .map(|c| c.chunk_text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n---\n"),
            )
        };
        let ctx_ref = context_str.as_deref();

        let reasoning_result = display::with_spinner("reasoning", || {
            self.agent.run_reasoning(&self.history, ctx_ref)
        });

        match reasoning_result {
            Err(e) => {
                display::error(&format!("Reasoning failed: {e}"));
                self.history.pop();
            }
            Ok(analysis) => {
                display::print_section(
                    "Reasoning",
                    display::BOLD_YELLOW,
                    display::DIM_YELLOW,
                    &analysis,
                );

                let decision_result = display::with_spinner("deciding", || {
                    self.agent.run_decision(&self.history, &analysis, ctx_ref)
                });

                match decision_result {
                    Ok(reply) => {
                        display::print_section(
                            "Answer",
                            display::BOLD_CYAN,
                            display::CYAN,
                            &reply,
                        );
                        self.history.push(ChatMessage {
                            role: "assistant".to_string(),
                            content: reply,
                        });
                    }
                    Err(e) => {
                        display::error(&format!("Decision failed: {e}"));
                        self.history.pop();
                    }
                }
            }
        }
    }
}
