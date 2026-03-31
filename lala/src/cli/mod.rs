mod commands;
mod display;
mod ingest;

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

use crate::agent::model::{ApiClient, ChatMessage, RouteDecision};
use crate::agent::planner::{Agent, needs_reasoning};
use rag::RagStore;

use commands::CommandResult;

const SYSTEM_PROMPT: &str =
    "You are a friendly AI assistant named lala. \
     Explain things clearly and naturally. \
     Respond in full sentences.";

pub fn run(api_url: &str, smart_router: bool, store: RagStore) -> anyhow::Result<()> {
    let client = ApiClient::new(api_url);
    let agent = Agent::new(&client);
    let mut rl = DefaultEditor::new()?;

    let mut history: Vec<ChatMessage> = vec![ChatMessage {
        role: "system".to_string(),
        content: SYSTEM_PROMPT.to_string(),
    }];

    // ── Welcome banner ────────────────────────────────────────────────────
    println!();
    let sep = "─".repeat(display::SECTION_WIDTH);
    println!("{}{}{}", display::DIM, sep, display::RESET);
    println!(
        "  {}lala{}  —  connected to {}{}{}",
        display::BOLD,
        display::RESET,
        display::CYAN,
        api_url,
        display::RESET,
    );
    if smart_router {
        println!(
            "  {}Router:{} LLM classifier",
            display::DIM,
            display::RESET,
        );
    }
    println!(
        "  Type {}/help{} for commands",
        display::DIM,
        display::RESET,
    );
    println!("{}{}{}", display::DIM, sep, display::RESET);
    println!();

    // ── REPL loop ─────────────────────────────────────────────────────────
    loop {
        let line = match rl.readline(">> ") {
            Ok(l) => l,
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(e) => return Err(e.into()),
        };

        let input = line.trim().to_string();
        if input.is_empty() {
            continue;
        }
        let _ = rl.add_history_entry(&input);

        // ── Command dispatch ──────────────────────────────────────────
        if input.starts_with('/') {
            match commands::dispatch(&input, &store) {
                CommandResult::Exit => break,
                CommandResult::Clear => {
                    history.truncate(1);
                    display::success("Conversation cleared.");
                    println!();
                    continue;
                }
                CommandResult::Handled => continue,
                CommandResult::NotACommand => {
                    display::warn(&format!("Unknown command: {input}"));
                    display::info("Type /help for available commands.");
                    println!();
                    continue;
                }
            }
        }

        // ── Chat path ────────────────────────────────────────────────
        history.push(ChatMessage {
            role: "user".to_string(),
            content: input.clone(),
        });

        let route = if smart_router {
            agent.classify_query(&input, &history)
        } else if needs_reasoning(&input) {
            RouteDecision::Reasoning
        } else {
            RouteDecision::Direct
        };

        if route == RouteDecision::Direct {
            let result = display::with_spinner("thinking", || agent.run_direct(&history));
            match result {
                Ok(reply) => {
                    display::print_section("Answer", display::BOLD_CYAN, display::CYAN, &reply);
                    history.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: reply,
                    });
                }
                Err(e) => {
                    display::error(&format!("Error: {e}"));
                    history.pop();
                }
            }
            continue;
        }

        // ── Reasoning → Decision pipeline ────────────────────────────
        let reasoning_result =
            display::with_spinner("reasoning", || agent.run_reasoning(&history));

        match reasoning_result {
            Err(e) => {
                display::error(&format!("Reasoning failed: {e}"));
                history.pop();
                continue;
            }
            Ok(analysis) => {
                display::print_section(
                    "Reasoning",
                    display::BOLD_YELLOW,
                    display::DIM_YELLOW,
                    &analysis,
                );

                let decision_result =
                    display::with_spinner("deciding", || agent.run_decision(&history, &analysis));

                match decision_result {
                    Ok(reply) => {
                        display::print_section(
                            "Answer",
                            display::BOLD_CYAN,
                            display::CYAN,
                            &reply,
                        );
                        history.push(ChatMessage {
                            role: "assistant".to_string(),
                            content: reply,
                        });
                    }
                    Err(e) => {
                        display::error(&format!("Decision failed: {e}"));
                        history.pop();
                    }
                }
            }
        }
    }

    println!("Bye!");
    Ok(())
}
