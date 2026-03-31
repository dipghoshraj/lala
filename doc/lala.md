# lala — CLI Client

> **Location:** `lala.ai/lala/`  
> **Role:** User-facing layer — interactive terminal REPL that sends conversation turns to the LLML API and displays responses with a live spinner.

---

## Overview

`lala` is the front-end of the system. It owns the user experience: readline input, multi-turn conversation history, a spinner animation during inference, and clean error recovery. It has no direct knowledge of the model — all LLM communication goes through HTTP to the LLML server.

```
User (terminal)
      │
  rustyline REPL
      │
  conversation history (in-memory)
      │
  POST /v1/chat/completions  ──►  LLML API server
      │
  spinner thread  (while waiting)
      │
  print response
```

---

## Source Layout

```
lala/src/
  main.rs          # Entry point — resolves API URL + LALA_SMART_ROUTER flag, starts CLI
  cli.rs           # REPL loop, spinner, conversation history, router branching
  agent/
    mod.rs
    model.rs       # ApiClient — HTTP wrapper (chat, reason, decide, classify); RouteDecision enum
    planner.rs     # Agent — classify_query(), run_direct(), run_reasoning(), run_decision()
```

---

## Running

```sh
cd lala

# Default — connects to http://localhost:3000
cargo run

# Custom server URL via argument
cargo run -- http://192.168.1.10:3000

# Custom server URL via environment variable
LLML_API_URL=http://192.168.1.10:3000 cargo run

# Enable LLM-based smart query router (requires LLML server)
LALA_SMART_ROUTER=1 cargo run
LALA_SMART_ROUTER=1 LLML_API_URL=http://192.168.1.10:3000 cargo run
```

URL resolution priority: **CLI argument → `LLML_API_URL` env var → `http://localhost:3000`**

`LALA_SMART_ROUTER=1` enables the LLM meta-classifier (`POST /v1/classify`) for each turn. When unset, a fast local keyword heuristic is used instead (no extra network call per turn).

---

## CLI Commands

| Input     | Action |
|-----------|--------|
| Any text  | Send as a user message to the LLM |
| `/clear`  | Reset conversation history (keeps system prompt) |
| `/exit`   | Quit |
| Ctrl-C / Ctrl-D | Quit |

Arrow-key history navigation (up/down) is provided by `rustyline`.

---

## Conversation History

The full message history is maintained in memory for the duration of the session. Each turn appends a `user` and `assistant` message:

```
[ system prompt, user1, assistant1, user2, assistant2, ... ]
```

The entire history is sent with every request so the model maintains context across turns. `/clear` trims the list back to just the system prompt, starting a fresh conversation without restarting the process.

If the API returns an error, the pending `user` message is removed from history so the context stays consistent.

---

## Spinner

While waiting for the API response, a braille spinner runs on a background thread:

```
  ⠼ thinking...
```

The spinner is stopped and the line is erased before the response is printed, so the output is clean:

```
>> What is Rust?
  ⠼ thinking...          ← live, while waiting
Rust is a systems programming language...   ← replaces the spinner line
```

Implementation: `Arc<AtomicBool>` stop flag shared between the main thread and the spinner thread. When the API call returns (success or error), the flag is set to `false`, the spinner thread exits, and the cursor line is cleared with spaces before any output is printed.

---

## HTTP Client — `agent/model.rs`

### `ApiClient`

```rust
ApiClient::new(base_url: &str) -> ApiClient
```

Wraps a `reqwest::blocking::Client` configured with **no timeout** — CPU inference can take tens of seconds and must not be aborted prematurely.

| Method | Endpoint | Description |
|--------|----------|-------------|
| `chat(messages, max_tokens, model)` | `/v1/chat/completions` | Core call — sends full history, returns reply string |
| `reason(messages, max_tokens)` | `/v1/chat/completions` | Shortcut with `model: "reasoning"` |
| `decide(messages, max_tokens)` | `/v1/chat/completions` | Shortcut with `model: "decision"` |
| `classify(query, context)` | `/v1/classify` | Returns `RouteDecision::{Direct,Reasoning}` |

### `ChatMessage`

```rust
pub struct ChatMessage {
    pub role: String,    // "system" | "user" | "assistant"
    pub content: String,
}
```

Shared between the CLI history and all HTTP request bodies.

### `RouteDecision`

```rust
pub enum RouteDecision { Direct, Reasoning }
```

Returned by `ApiClient::classify()` and by `Agent::classify_query()`. Defaults to `Reasoning` on any parse error (fail-closed).

---

## Query Router — `agent/planner.rs`

Every user turn goes through a routing decision before inference:

```
input query
    │
    ▼
 classify_query(input, history)
    │
    ├── LALA_SMART_ROUTER=1  →  POST /v1/classify  → RouteDecision
    └── heuristic (default)  →  needs_reasoning(input) → RouteDecision
    │
    ├── Direct     →  run_direct(history)             → decision model only
    └── Reasoning  →  run_reasoning(history)          → reasoning model
                        run_decision(history, analysis) → decision model
```

- `needs_reasoning()` — local keyword + word-count heuristic; used as fallback when server is unreachable or smart router is off.
- `classify_query()` — calls `client.classify()`, falls back to `needs_reasoning()` on any `Err`.
- Reasoning output is displayed to the user under a `▷ Reasoning` section (yellow ANSI).

---

## System Prompt

The system prompt is hardcoded in `cli.rs` and always occupies index 0 of the history:

```
You are a friendly AI assistant named lala.
Explain things clearly and naturally.
Respond in full sentences.
```

This can be edited in `cli.rs` under `const SYSTEM_PROMPT`.

---

## Dependencies

| Crate       | Purpose |
|-------------|---------|
| `reqwest`   | Blocking HTTP client for LLML API calls |
| `rustyline` | Readline-style input with history and arrow-key navigation |
| `serde` / `serde_json` | HTTP request/response serialization |
| `anyhow`    | Error propagation |
| `rag` (path dep) | Standalone RAG crate — SQLite FTS5 store + retrieve |

---

## System Architecture

`lala` communicates with the LLML server over HTTP. Both the query classification and inference happen server-side.

```
┌─────────────┐          POST /v1/classify            ┌──────────────────┐
│   lala CLI  │  ──────────────────────────────────►   LLML server    │
│  (lala/)    │  POST /v1/chat/completions         │   (LLML/)         │
│             │  ◄──────────────────────────────────   │                  │
│  User REPL  │  JSON response                     │  llama-cpp-python │
└─────────────┘                                   └──────────────────┘
```

See [LLML-py.md](LLML-py.md) for the server-side documentation.
