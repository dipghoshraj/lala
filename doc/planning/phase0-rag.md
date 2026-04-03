# Phase 0 — RAG Layer

> **Status:** Complete
> **Depends on:** [phase0.md](phase0.md) — layered architecture scaffold
> **Goal:** Standalone Rust `rag` crate using SQLite FTS5 for keyword (BM25) retrieval, consumed by the `lala` CLI. Includes structured memory blocks, RSS news ingestion, and live agent context injection. No neural embeddings. Qdrant vector search is Phase 1.

---

## 1. How to Use

### Start the CLI

```sh
cd lala && cargo run                          # connects to http://localhost:3000
LLML_API_URL=http://192.168.1.10:3000 cargo run   # custom server
```

RAG is built into the REPL. Type `/help` to see all commands.

### Ingest Documents

**Option A — Batch ingest (recommended)**

Place files in the `./ingest/` directory and run:

```
>> /ingest
  ℹ Found 3 file(s) in ./ingest/
  [1/3] architecture.md
  ✓ architecture.md → 12 chunks
  [2/3] phase0.md
  ✓ phase0.md → 8 chunks
  [3/3] notes.txt
  ⚠ notes.txt: Already ingested: ./ingest/notes.txt
  ────────────────────────────────────────────────────────────
  Ingested: 2  Skipped: 1  Failed: 0  Chunks: 20
  ────────────────────────────────────────────────────────────
```

The ingest directory defaults to `./ingest/` and is created automatically on first run. Override with `LALA_INGEST_DIR`.

**Option B — Single file**

```
>> /ingest-file doc/architecture.md
  ✓ architecture.md → 12 chunks
```

### Search

```
>> /search layered architecture
  ────────────────────────────────────────────────────────────
  [1] score: -3.2140  chunk #2
      The system is structured as five distinct layers. Data only flows between adjacent…

  [2] score: -1.8700  chunk #0
      lala.ai — Agentic RAG System. Rust-based local Agentic RAG system…
  ────────────────────────────────────────────────────────────
```

Returns the top 5 chunks ranked by BM25 relevance. More negative score = better match.

### Check Status

```
>> /status
  ────────────────────────────────────────────────────────────
  Documents: 3    Chunks: 28
  Ingest dir: ./ingest
  ────────────────────────────────────────────────────────────
```

### All CLI Commands

| Command | Behaviour |
|---------|-----------|
| `/ingest` | Batch-ingest all files in `./ingest/` (or `LALA_INGEST_DIR`) |
| `/ingest-file <path>` | Ingest a single file by explicit path |
| `/ingest-news <rss_url>` | Fetch an RSS feed and ingest all linked articles |
| `/search <query>` | BM25 full-text search over ingested chunks (top 5) |
| `/memory-search <query>` | BM25 search over structured memory blocks (top 5) |
| `/status` | Show document count, chunk count, ingest directory |
| `/help` | Show available commands |
| `/clear` | Reset conversation history |
| `/exit` | Quit |

---

## 2. How It Works

### Data Flow — Ingestion (File)

```
/ingest  or  /ingest-file doc/architecture.md
      │
      ▼
  cli/ingest.rs — scan directory or read single file
      │  std::fs::read_to_string(path)
      ▼
  rag::chunk(text, 512, 64)  →  Vec<String>   (N chunks)
      │  512-char windows, 64-char overlap
      │
      ├─► INSERT INTO documents (id, title, source, created_at)
      │
      ├─► INSERT INTO chunks_fts (chunk_id, document_id, chunk_index, chunk_text, char_count)
      │       × N rows   (inside a single transaction)
      │
      └─► build_memory_block(chunk_text)  → (facts, capabilities, constraints)   [placeholder]
              INSERT INTO memory_blocks (...)
              × N rows   (same transaction)
```

### Data Flow — RSS News Ingestion

```
/ingest-news https://feeds.bbci.co.uk/news/rss.xml
      │
      ▼
  cli/ingest.rs → rag::ingest_news_feed(store, rss_url, delay_ms=1000)
      │
      ▼
  rss::Channel::read_from(feed bytes)
      │
  for each item:
      ├─ GET article HTML (with User-Agent; HTTP 403 → CORS proxy fallback)
      ├─ extract_text_from_html(html) → plain text
      ├─ store.ingest(title, article_url, text)  (deduped by URL)
      └─ sleep(delay_ms)
```

### Data Flow — Retrieval

```
/search layered architecture
      │
      ▼
  SELECT chunk_id, document_id, chunk_index, chunk_text, bm25(chunks_fts) AS score
  FROM   chunks_fts
  WHERE  chunk_text MATCH 'layered architecture'
  ORDER  BY bm25(chunks_fts)
  LIMIT  5
      │
      ▼
  Vec<Chunk>  →  printed as ranked previews
```

FTS5's `bm25()` returns a **negative** float — more negative = better match.

### Data Flow — Memory Block Retrieval

```
/memory-search system constraints
      │
      ▼
  SELECT b.id, ... b.facts, b.capabilities, b.constraints ...
  FROM   chunks_fts c
  JOIN   memory_blocks b ON b.document_id = c.document_id AND b.chunk_index = c.chunk_index
  WHERE  c.chunk_text MATCH 'system OR constraints'
  ORDER  BY bm25(chunks_fts)
  LIMIT  5
      │
      ▼
  Vec<MemoryBlock>  →  printed with FACTS / CAPABILITIES / CONSTRAINTS
```

### Data Flow — Agent Context Injection (Live)

Every query through the `lala` REPL automatically retrieves and injects RAG context before calling the model:

```
User query
      │
      ▼
  agent::retrieve_context(query)
       ├─ sanitise query (strip FTS5 special chars)
       ├─ join terms with " OR "
       └─ store.retrieve(fts_query, 5) → Vec<Chunk>
      │
  agent::retrieve_memory_context(query)  → Vec<MemoryBlock>
      │
  limit_chunks_by_tokens(chunks, 800)    (token budget enforcement)
  limit_memory_by_tokens(memory, budget)
      │
  context_str = join chunk_texts + memory facts/caps/constraints
      │
  inject into system prompt as
  "--- Retrieved Context ---\n{context_str}\n--- End Context ---"
      │
  POST /v1/chat/completions  (with context in system prompt)
```

### Error Handling

| Scenario | Behaviour |
|----------|-----------|
| File not found | `✗ cannot read file: ...`, continue REPL |
| Empty file | `⚠ file is empty`, skipped |
| Duplicate source | `⚠ Already ingested: <path>`, skipped |
| Empty search query | `Usage: /search <query>` |
| No search results | `⚠ No results found for: <query>` |
| RSS fetch HTTP 403 | Retry via CORS proxy (`api.allorigins.win`) |
| RSS article empty text | `✗ Empty text extracted`, failed count |
| Unknown command | `⚠ Unknown command: ...` + hint to use `/help` |

---

## 3. Architecture

### Module Layout

```
lala.ai/
  Cargo.toml                  ← Workspace root: members = ["lala", "rag"], resolver = "3"
  Cargo.lock                  ← Shared lockfile (auto-generated)

  rag/                        ← Standalone RAG library crate
    Cargo.toml                ← deps: rusqlite (bundled), uuid (v4), anyhow, reqwest (blocking), rss, regex, urlencoding
    src/
      lib.rs                  ← RagStore, Chunk, MemoryBlock, store(), ingest(), retrieve(), retrieve_memory_blocks(),
                                  memory_blocks_for_document(), memory_blocks_for_source(), update_memory_block(),
                                  document_count(), chunk_count(), is_prose_content()
      chunker.rs              ← chunk(text, chunk_size, overlap) → Vec<String>
      news.rs                 ← ingest_news_feed(store, rss_url, delay_ms) → (ingested, skipped, failed)

  lala/                       ← CLI + Agent binary crate
    Cargo.toml                ← deps include: rag = { path = "../rag" }
    src/
      main.rs                 ← Startup: resolve API URL + DB path, init RagStore, start CLI
      cli/
        mod.rs                ← REPL loop, animated banner, command/chat dispatch
        chat.rs               ← Chat struct — history, retrieve+inject context, spinner, handle()
        commands.rs           ← Command dispatch (/help, /status, /search, /memory-search, /ingest, /ingest-news)
        ingest.rs             ← Batch + single-file + RSS news ingestion with progress output
        display.rs            ← Spinner, colours, print_section(), print_sources(), helpers
      agent/
        mod.rs
        model.rs              ← ApiClient — HTTP wrapper
        planner.rs            ← Agent — query router + reasoning→decision pipeline + retrieve_context()
```

### Decisions

| Concern | Decision | Rationale |
|---------|----------|-----------|
| Embedding | None (Phase 0; Qdrant in Phase 1) | Avoid model dependency; FTS5 BM25 delivers sufficient keyword recall for Phase 0 |
| Retrieval engine | SQLite FTS5 | Bundled with `rusqlite`; no external service; FTS5 exposes native BM25 ranking |
| DB file | `./lala.db` (next to binary) | Simple default; overridable via `LALA_DB_PATH` env var |
| DB crate | `rusqlite` with `bundled` feature | Compiles SQLite + FTS5 in; no system SQLite install required |
| ID generation | `uuid` v4 | Collision-free identifiers for documents, chunks, and memory blocks |
| Chunker | Character-based sliding window | 512-char chunks, 64-char overlap — simple, no tokeniser dependency |
| Duplicate handling | Skip if `(source)` already exists in `documents` | Prevents duplicate chunks and memory blocks |
| Ingest directory | `./ingest/` (configurable) | Batch ingestion of multiple files from a known location |
| Memory blocks | Placeholder extraction (facts = capabilities = constraints = chunk_text) | LLM-based extraction deferred to Phase 1; structure is in place |
| Agent RAG wiring | Live in every query (direct + reasoning paths) | Context injected up to 800-token budget; improves response grounding immediately |
| News ingestion | Separate `news.rs` module | Extensible source pattern — RSS today, other sources in later phases |
| CLI modularity | `cli/` directory with submodules | Each file is focused, testable, maintainable |

### Schema

```sql
CREATE TABLE IF NOT EXISTS documents (
    id         TEXT PRIMARY KEY,
    title      TEXT NOT NULL,
    source     TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    chunk_id    UNINDEXED,
    document_id UNINDEXED,
    chunk_index UNINDEXED,
    chunk_text,
    char_count  UNINDEXED
);

CREATE TABLE IF NOT EXISTS memory_blocks (
    id            TEXT PRIMARY KEY,
    document_id   TEXT NOT NULL,
    chunk_index   INTEGER NOT NULL,
    chunk_text    TEXT NOT NULL,
    facts         TEXT NOT NULL,
    capabilities  TEXT NOT NULL,
    constraints   TEXT NOT NULL,
    created_at    TEXT NOT NULL
);
```

All three tables are created via `CREATE ... IF NOT EXISTS` inside `RagStore::open()`. `memory_blocks` rows are inserted in the same transaction as `chunks_fts` rows during every `store()` call.

### Public API (rag crate)

```rust
use rag::{RagStore, Chunk, MemoryBlock};

let store = RagStore::open("./lala.db")?;

// Ingest: chunk text, store in SQLite FTS5, auto-extract memory blocks
let count = store.store("title", "source_path", "text content")?; // → chunk count
let count = store.ingest("title", "source_path", "text content")?; // alias for store()

// Retrieve: BM25 full-text search, top-k results
let chunks: Vec<Chunk> = store.retrieve("search query", 5)?;
let blocks: Vec<MemoryBlock> = store.retrieve_memory_blocks("query", 5)?;

// Lookup by document ID or source path
let blocks: Vec<MemoryBlock> = store.memory_blocks_for_document("doc-uuid")?;
let blocks: Vec<MemoryBlock> = store.memory_blocks_for_source("./ingest/file.md")?;

// Update extracted memory for a block (e.g. from LLM extraction in Phase 1)
store.update_memory_block("block-uuid", "facts text", "capabilities text", "constraints text")?;

// Stats
let docs: usize = store.document_count()?;
let chunks: usize = store.chunk_count()?;

// Free functions exported from the crate
let parts: Vec<String> = rag::chunk(text, 512, 64);         // sliding window chunker
let (i, s, f): = rag::ingest_news_feed(&store, rss_url, 1000_u64)?;  // RSS ingestion
let prose: bool = rag::is_prose_content(text);               // prose vs code heuristic
```

#### Data Structures

```rust
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub chunk_index: usize,
    pub chunk_text: String,
    pub score: f64,        // BM25 rank — negative; more negative = better match
    pub title: String,
    pub source: String,
}

pub struct MemoryBlock {
    pub id: String,
    pub document_id: String,
    pub chunk_index: usize,
    pub chunk_text: String,
    pub facts: String,         // Phase 0: same as chunk_text (placeholder)
    pub capabilities: String,  // Phase 1: LLM-extracted capabilities
    pub constraints: String,   // Phase 1: LLM-extracted constraints
    pub title: String,
    pub source: String,
}
```

### Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `LALA_DB_PATH` | `./lala.db` | SQLite database file path |
| `LALA_INGEST_DIR` | `./ingest` | Directory scanned by `/ingest` |
| `LLML_API_URL` | `http://localhost:3000` | LLML inference server URL |
| `LALA_SMART_ROUTER` | unset | Set to `1` to enable LLM-based query classification |

---

## 4. Crate Independence

The `rag` crate has zero dependencies on `lala`, the agent, CLI, or model layers. Any application in the workspace can depend on it:

- `lala` consumes it via `rag = { path = "../rag" }` in `Cargo.toml`
- Future applications (HTTP API, background indexer, telegram Rust port) can independently depend on the `rag` crate
- The retrieval backend can be swapped (e.g. to Qdrant) without changing any consumer code, as long as the `RagStore` method signatures are preserved

### rag crate dependencies

| Crate | Purpose |
|-------|---------|
| `rusqlite` (bundled) | SQLite + FTS5 for BM25 keyword retrieval |
| `uuid` (v4) | Document, chunk, and memory block ID generation |
| `anyhow` | Error propagation |
| `reqwest` (blocking) | HTTP client for RSS article fetching |
| `rss` | RSS/Atom feed parsing |
| `regex` | HTML text extraction |
| `urlencoding` | CORS-proxy URL encoding fallback |

---

## 5. Agent RAG Wiring (Live)

RAG context is injected into **every** LLM call in `lala/src/cli/chat.rs`. This was targeted for Phase 1 but was completed in Phase 0. The wiring involves two retrieval calls plus token budget enforcement:

```rust
// Executed on every query before calling the model:
let chunks  = agent.retrieve_context(&input)?;         // Vec<Chunk>
let memory  = agent.retrieve_memory_context(&input)?;  // Vec<MemoryBlock>
let chunks  = limit_chunks_by_tokens(chunks, CONTEXT_TOKEN_BUDGET);
let memory  = limit_memory_by_tokens(memory, remaining_budget);
let context = build_context_str(&chunks, &memory);     // injected into system prompt
```

`retrieve_context()` in `planner.rs` sanitises the query (strip FTS5 special characters), splits words, joins with `" OR "`, and calls `store.retrieve()`. This gives broad recall: a query like `"explain lala front end"` becomes `"explain OR lala OR front OR end"`.

`limit_chunks_by_tokens()` and `limit_memory_by_tokens()` use a 3-bytes-per-token estimate to ensure the context block never overflows the model's context window (`CONTEXT_TOKEN_BUDGET = 800` tokens).

---

## 6. Phase 1 — Qdrant Vector Search Migration

Phase 1 replaces the SQLite FTS5 backend with **Qdrant** for dense vector (semantic) similarity search. The public `RagStore` API is preserved unchanged; only the internal implementation changes.

### Migration Scope

| Component | Phase 0 (current) | Phase 1 (Qdrant) |
|-----------|-------------------|------------------|
| `rag/Cargo.toml` | `rusqlite` (bundled) | Add `qdrant-client`; keep `rusqlite` optionally for dedup tracking |
| `rag/src/lib.rs` | `Connection` + FTS5 SQL | Qdrant HTTP/gRPC client; `store()` calls `/v1/embed` then upserts to collection |
| `rag/src/lib.rs` | `retrieve()` BM25 ranking | `retrieve()` embeds query via `/v1/embed`, calls Qdrant vector search |
| `lala/src/agent/planner.rs` | FTS5 query string (`OR` join) | Pass raw query to `store.retrieve()` (embedding happens inside rag crate) |
| `LLML/api/routes.py` | inference only | Add `POST /v1/embed` endpoint |
| `ai-config.yaml` | 2 model roles | Add `embedding` role (e.g. `bge-small-en-v1.5`) |
| Qdrant server | not needed | `docker run qdrant/qdrant` on port 6333; config via `LALA_QDRANT_URL` |

### Qdrant Collection Schema (planned)

```
Collection: "lala_chunks"
  vector: { size: <embedding_dim>, distance: Cosine }
  payload per point:
    chunk_id     (keyword)
    document_id  (keyword)
    chunk_index  (integer)
    chunk_text   (text)
    title        (keyword)
    source       (keyword)
    char_count   (integer)

Collection: "lala_memory"
  vector: { size: <embedding_dim>, distance: Cosine }
  payload per point:
    block_id      (keyword)
    document_id   (keyword)
    chunk_index   (integer)
    facts         (text)
    capabilities  (text)
    constraints   (text)
    source        (keyword)
```

Duplicate detection on `source` field uses a Qdrant payload filter (`must: [{key: "source", match: {value: url}}]`) before upserting.

### Unchanged Interfaces After Migration

- `store.store()`, `store.retrieve()`, `store.retrieve_memory_blocks()` — same signatures
- All CLI commands (`/ingest`, `/ingest-news`, `/search`, `/memory-search`) — unchanged
- Agent wiring in `chat.rs` / `planner.rs` — unchanged
- LLML server boundary — LLML stays stateless; Qdrant client lives in `rag` crate only

---

## 7. Out of Scope (Remaining)

| Feature | Target phase |
|---------|--------------|
| LLM-based memory block extraction (populate facts/capabilities/constraints via `/v1/chat/completions`) | Phase 1 |
| Vector similarity search (Qdrant) replacing FTS5 BM25 | Phase 1 |
| Embedding endpoint (`POST /v1/embed`) in LLML | Phase 1 |
| Hybrid BM25 + vector reranking | Phase 2 |
| Metadata filtering in Qdrant | Phase 2 |
| Session-scoped retrieval | Phase 1 |
| Embedding model version tagging / re-embedding on upgrade | Phase 2 |
