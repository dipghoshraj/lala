# RAG Layer — lala.ai

> Retrieval-Augmented Generation implementation. SQLite FTS5 keyword retrieval (Phase 0). Qdrant vector search planned for Phase 1.

---

## 1. Overview

The **RAG (Retrieval-Augmented Generation) Layer** is a standalone Rust library crate (`rag/`) that handles all knowledge storage, chunking, retrieval, and memory block operations. It provides context injection to the agent loop, enabling the model to cite evidence from ingested documents.

**Phase 0 (Current):** SQLite FTS5 (keyword BM25 retrieval), fixed-size chunking, placeholder memory block extraction.

**Phase 1 (Planned):** Qdrant vector search (semantic similarity), embedding model integration, LLM-based memory block extraction.

The public API (`RagStore` methods) remains **unchanged** between phases; only the internal retrieval backend swaps.

---

## 2. Crate Structure

```
rag/
├── Cargo.toml             # deps: rusqlite(bundled), rss, reqwest, uuid, regex, anyhow
└── src/
    ├── lib.rs             # RagStore (public API), Chunk, MemoryBlock structs
    ├── chunker.rs         # chunk(text, size, overlap) → Vec<String>
    └── news.rs            # ingest_news_feed(store, rss_url, delay) RSS parser
```

### Dependencies

| Crate | Version | Why |
|-------|---------|-----|
| `rusqlite` + `bundled` | ~0.31 | Embedded SQLite + FTS5 (no external service) |
| `uuid` | ~1.0 | Document/chunk ID generation (v4 random) |
| `anyhow` | ~1.0 | Error propagation (? operator) |
| `reqwest` (blocking) | ~0.12 | HTTP client for RSS fetching, CORS fallback |
| `rss` | ~0.13 | RSS feed parsing (channel → items) |
| `regex` | ~1.10 | HTML tag stripping, URL dedup patterns |

---

## 3. Public API Reference

### RagStore Methods

All methods operate on `RagStore` instance created via `RagStore::open(db_path)`.

#### **Initialization**
```rust
pub fn open(db_path: impl AsRef<Path>) -> Result<Self>
```
- Creates or opens SQLite DB at `db_path`
- Auto-creates three tables if missing: `documents`, `chunks_fts`, `memory_blocks`
- Returns `RagStore` instance with connection pool
- Fails if DB file not writable or schema creation fails

---

#### **Document Storage**
```rust
pub fn store(&mut self, title: &str, source: &str, text: &str) -> Result<(String, usize)>
```
- Divides `text` into chunks using `chunk()` (512 chars default, 64-char overlap)
- Inserts `document` record with `title` and `source` (unique constraint on source)
- Inserts each chunk into `chunks_fts` virtual table for FTS5 indexing
- Creates placeholder `memory_block` for each chunk with `facts=capabilities=constraints=chunk_text`
- Returns `(document_id, chunk_count)`
- On duplicate `source`: skips and returns existing doc ID

**Usage:**
```rust
let (doc_id, chunks) = store.store(
    "README",
    "https://example.com/readme.md",
    "# My Project\nThis is a long document..."
)?;
println!("Stored {} chunks", chunks);
```

---

#### **Keyword Retrieval**
```rust
pub fn retrieve(&self, query: &str, k: usize) -> Result<Vec<Chunk>>
```
- Parses query into words, joins with " OR " for broad recall
- Executes FTS5 BM25 search on `chunks_fts` table
- Returns top-k chunks ranked by relevance score (negative; more negative = better match)
- Each `Chunk` contains: `id`, `document_id`, `chunk_index`, `chunk_text`, `score`

**Example:**
```rust
let results = store.retrieve("Rust ownership rules", 5)?;
for chunk in results {
    println!("Score: {}, Text: {:?}", chunk.score, chunk.chunk_text[..50].to_string());
}
```

---

#### **Memory Block Retrieval**
```rust
pub fn retrieve_memory_blocks(&self, query: &str, k: usize) -> Result<Vec<MemoryBlock>>
```
- Same search semantics as `retrieve()`, but joins with `memory_blocks` table
- Returns metadata-enriched results with `facts`, `capabilities`, `constraints` fields
- Useful for extracting semantic structure (candidates for Phase 1 LLM extraction)

**MemoryBlock struct:**
```rust
pub struct MemoryBlock {
    pub id: String,
    pub document_id: String,
    pub chunk_index: usize,
    pub chunk_text: String,
    pub facts: String,           // Phase 0: same as chunk_text
    pub capabilities: String,    // Phase 0: same as chunk_text
    pub constraints: String,     // Phase 0: same as chunk_text
    pub created_at: String,
}
```

---

#### **Memory Block Lookups**
```rust
pub fn memory_blocks_for_document(&self, doc_id: &str) -> Result<Vec<MemoryBlock>>
pub fn memory_blocks_for_source(&self, path: &str) -> Result<Vec<MemoryBlock>>
```
- Retrieve all memory blocks for a specific document (by ID or source path)
- Useful for re-indexing or batch updates in Phase 1

---

#### **Memory Block Updates** (Phase 1 Preparation)
```rust
pub fn update_memory_block(
    &mut self,
    id: &str,
    facts: &str,
    capabilities: &str,
    constraints: &str,
) -> Result<()>
```
- Updates `facts`, `capabilities`, `constraints` fields for a memory block
- Used in Phase 1 when LLM extraction replaces placeholder logic
- No-op in Phase 0 (placeholder extraction runs on `store()`)

---

#### **Statistics**
```rust
pub fn document_count(&self) -> Result<usize>
pub fn chunk_count(&self) -> Result<usize>
```
- Count stored documents and chunks
- Used for CLI info display (`/search` command)

---

### Free Functions

#### **Chunking**
```rust
pub fn chunk(text: &str, chunk_size: usize, overlap: usize) -> Vec<String>
```
- Splits text into fixed-size chunks with sliding overlap
- Edge cases:
  - Empty text → returns `Vec::new()`
  - Text shorter than `chunk_size` → returns `vec![text.to_string()]`
  - `overlap >= chunk_size` → no overlap (safety check)
- **Algorithm:** Character-based sliding window (not token-aware; token counts computed upstream)

**Example:**
```rust
let chunks = rag::chunk(
    "Hello world! This is a test.",
    10,   // chunk_size
    3     // overlap
);
// Result: ["Hello worl", "orld! This", "s is a tes", "est."]
```

---

#### **News Ingestion**
```rust
pub async fn ingest_news_feed(
    store: &mut RagStore,
    rss_url: &str,
    delay_ms: u64,
) -> Result<(usize, usize, usize)>
```
- Fetches RSS feed from `rss_url`
- For each item: fetch article HTML → extract text → check for duplicates (by URL) → ingest
- Returns tuple: `(successfully_ingested, skipped_duplicates, failed_fetches)`

**Flow:**
1. `reqwest::blocking::Client::get(rss_url)` → response text
2. Parse as RSS with `rss::Channel::read_from()`
3. For each `<item>`:
   - Extract `link` and `title`
   - Fetch HTML from link: `GET link` (with User-Agent)
   - **HTTP 403 fallback:** Retry via CORS proxy `api.allorigins.win/get?url={link}`
   - Extract text from HTML: strip tags with regex `<[^>]+>`
   - Check if URL already in `documents.source` (skip if exists)
   - `store.store(title, link, article_text)`
4. Sleep `delay_ms` between fetches (politeness)

**Example:**
```rust
let (ingested, skipped, failed) = rag::ingest_news_feed(
    &mut store,
    "https://news.python.org/feed.rss",
    1000  // 1-second delay
).await?;
println!("Ingested: {}, Skipped: {}, Failed: {}", ingested, skipped, failed);
```

---

## 4. Data Structures

### Chunk

```rust
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub chunk_index: usize,
    pub chunk_text: String,
    pub score: f64,  // FTS5 BM25 score; negative; -5.0 better than -2.0
}
```

- `id`: UUID v4, unique per chunk
- `score`: Negative float from FTS5 BM25 algorithm; more negative = better match
- Returned by `retrieve(query, k)`

### MemoryBlock

```rust
pub struct MemoryBlock {
    pub id: String,
    pub document_id: String,
    pub chunk_index: usize,
    pub chunk_text: String,
    pub facts: String,
    pub capabilities: String,
    pub constraints: String,
    pub created_at: String,     // ISO 8601: YYYY-MM-DD HH:MM:SS
}
```

- One per chunk (created during `store()`)
- **Phase 0:** `facts=capabilities=constraints=chunk_text` (placeholder)
- **Phase 1:** LLM extracts semantic fields from `chunk_text`
- Returned by `retrieve_memory_blocks(query, k)`

---

## 5. Database Schema

SQLite FTS5 (Full-Text Search) with three tables:

### documents
```sql
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    source TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL   -- ISO 8601
);
```
- One row per ingested file/RSS feed
- `source`: Unique constraint prevents re-ingesting same file
- Example source: `/path/to/file.md` or `https://example.com/article`

### chunks_fts (FTS5 Virtual Table)
```sql
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    chunk_id UNINDEXED,
    document_id UNINDEXED,
    chunk_index UNINDEXED,
    chunk_text,        -- Full-text indexed
    char_count UNINDEXED
);
```

- FTS5 virtual table for keyword (BM25) search on `chunk_text`
- UNINDEXED columns: stored but not searched (metadata only)
- Query: `SELECT * FROM chunks_fts WHERE chunks_fts MATCH 'Rust ownership'`
- Scoring: Native FTS5 BM25 algorithm (field weights: chunk_text only, no boosting)

### memory_blocks
```sql
CREATE TABLE memory_blocks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    chunk_text TEXT NOT NULL,
    facts TEXT NOT NULL,         -- Phase 0: = chunk_text
    capabilities TEXT NOT NULL,  -- Phase 0: = chunk_text
    constraints TEXT NOT NULL,   -- Phase 0: = chunk_text
    created_at TEXT NOT NULL,    -- ISO 8601
    FOREIGN KEY(document_id) REFERENCES documents(id)
);
```

- Parallel to chunks: one row per chunk
- Fields `facts`, `capabilities`, `constraints` are placeholders in Phase 0
- In Phase 1, `update_memory_block()` replaces these with LLM-extracted summaries

---

## 6. Chunking Strategy

### Fixed-Size Sliding Window (Phase 0)

```rust
pub fn chunk(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.is_empty() { return vec![]; }
    
    let mut chunks = Vec::new();
    let mut start = 0;
    
    while start < text.len() {
        let end = (start + chunk_size).min(text.len());
        chunks.push(text[start..end].to_string());
        
        if end == text.len() { break; }
        start += chunk_size - overlap;
    }
    
    chunks
}
```

**Characteristics:**
- **Character-based** (not token-aware in Phase 0)
- **Default:** 512 chars per chunk, 64-char overlap
- **Overlap:** Preserves context across chunk boundaries
- **Edge case:** If overlap ≥ chunk_size, effectively no overlap

**Example (simplified):**
```
Text: "A B C D E F G H"
Size: 4, Overlap: 1

Chunks:
  0: "A B C"
  1: "C D E" (overlap: 1 char = 'C')
  2: "E F G"
  3: "G H"
```

**Phase 1 (Planned):** Replace character-based with token-aware chunking using tokenizer from embedding model, respecting token window (e.g. 300 tokens per chunk, 50-token overlap).

---

## 7. Memory Block Extraction

### Phase 0 — Placeholder Extraction

During `store()`, each chunk gets a default `MemoryBlock`:

```rust
fn build_memory_block(chunk: &Chunk) -> MemoryBlock {
    MemoryBlock {
        id: uuid::Uuid::new_v4().to_string(),
        document_id: chunk.document_id.clone(),
        chunk_index: chunk.chunk_index,
        chunk_text: chunk.chunk_text.clone(),
        facts: chunk.chunk_text.clone(),           // Placeholder
        capabilities: chunk.chunk_text.clone(),    // Placeholder
        constraints: chunk.chunk_text.clone(),     // Placeholder
        created_at: chrono::Local::now().to_rfc3339_opts(...),
    }
}
```

**Why placeholders?**
- No embedding model or LLM extraction in Phase 0
- Allows `retrieve_memory_blocks()` to work without breaking change
- Phase 1 replaces via `update_memory_block()` after embedding + LLM processing

### Phase 1 — LLM-Based Extraction

Planned flow (not yet implemented):

1. **Embedding phase:**
   - Call `/v1/embed` (LLML) with chunk text
   - Receive `Vec<f32>` (384 or 768 dims, depending on model)

2. **Qdrant indexing:**
   - Insert embedded chunk into Qdrant collection `lala_chunks`
   - Store metadata: `document_id`, `chunk_index`

3. **Memory extraction:**
   - Prompt LLM: "Extract facts, capabilities, and constraints from this text: {chunk}"
   - Parse structured response (JSON or prompt-engineered format)
   - Call `update_memory_block()` to replace placeholders

**Rationale:**
- **Semantic grouping:** Similar chunks surface together even with different keywords
- **Structured knowledge:** Facts/capabilities/constraints reduce hallucination on cite-heavy tasks
- **Gradual rollout:** Phase 0 works with placeholder; Phase 1 improves without API change

---

## 8. News Ingestion

### RSS Parsing Flow

The `ingest_news_feed(store, rss_url, delay_ms)` function:

```
Fetch RSS URL
    ↓
Parse feed (rss >= 0.13)
    ↓
For each <item>:
  - Extract title, link
  - Fetch article HTML (GET with fallback to CORS proxy)
  - Strip HTML tags → extract text
  - Check if URL in documents.source (skip if exists)
  - store.store(title, link, article_text)
  - Sleep delay_ms
    ↓
Return (ingested, skipped, failed)
```

### CORS Fallback

Some RSS links return HTTP 403 (forbidden) when requested without specific headers:

1. **Primary attempt:** `reqwest::blocking::Client::get(link)` with User-Agent header
2. **Fallback on 403:** Retry via CORS proxy: `reqwest::get(format!("https://api.allorigins.win/get?url={}", link))`
   - Allorigins.win proxies the request and returns JSON: `{status: 200, contents: "<html>..."}`
   - Extract `.contents` and continue with HTML stripping

**Example response from Allorigins:**
```json
{
  "status": 200,
  "contents": "<html><body>Article text...</body></html>"
}
```

### Duplicate Handling

Before inserting, `store()` checks:
- If `source` (URL) already in `documents.source`, skip insertion
- Returns existing doc ID (no-op if already stored)

**Implication:** Re-running `ingest_news(same_rss_url)` will fetch all items again, but only new ones (by URL) are stored.

---

## 9. Token Budget & Agent Wiring

### Agent-RAG Integration

The agent layer (`lala/src/agent/planner.rs`) calls RAG on every query:

```rust
impl Agent {
    pub fn retrieve_context(&self, query: &str) -> Result<Vec<Chunk>> {
        let sanitized = sanitize_query(query);  // Remove control chars
        let tokens: Vec<&str> = sanitized.split_whitespace().collect();
        let or_query = tokens.join(" OR ");      // Broad recall
        self.store.retrieve(&or_query, 5)        // Top-5 chunks
    }
    
    pub fn retrieve_memory_context(&self, query: &str) -> Result<Vec<MemoryBlock>> {
        // Same as above, but returns memory blocks instead
        let sanitized = sanitize_query(query);
        let or_query = tokens.join(" OR ");
        self.store.retrieve_memory_blocks(&or_query, 5)
    }
}
```

### Token Limiting in Chat CLI

The CLI (`lala/src/cli/chat.rs`) enforces a 800-token budget:

```rust
fn limit_chunks_by_tokens(chunks: Vec<Chunk>, max_tokens: usize) -> Vec<Chunk> {
    let mut result = Vec::new();
    let mut total = 0;
    for chunk in chunks {
        let tokens = encode(&chunk.chunk_text).len();
        if total + tokens > max_tokens { break; }
        total += tokens;
        result.push(chunk);
    }
    result
}
```

**Flow:**
1. User types query
2. `retrieve_and_limit_context(query)` calls `agent.retrieve_context(query)` → top-5 chunks
3. Apply `limit_chunks_by_tokens()` with budget=800 → keep first N chunks that fit
4. Format as "--- Retrieved Context ---" block in system prompt
5. Both `run_direct()` and `run_reasoning()` receive context-augmented system prompt

---

## 10. Phase 1 Migration Checklist

### Overview

Phase 1 replaces 3 components with Qdrant equivalents while keeping the public API unchanged.

### Checklist

- [ ] **Embedding Model**
  - [ ] Add `embedding: bge-small-en-v1.5` role to `ai-config.yaml`
  - [ ] Update LLML `ModelRegistry` to load embedding model
  - [ ] Implement `POST /v1/embed` endpoint in `LLML/api/routes.py`
  - [ ] Test embedding endpoint with sample text

- [ ] **Qdrant Setup**
  - [ ] Update `Cargo.toml` in `rag/` crate: add `qdrant-client`
  - [ ] Provision Qdrant server (Docker or managed)
  - [ ] Add `LALA_QDRANT_URL` env var (e.g. `http://localhost:6333`)
  - [ ] Create two collections: `lala_chunks`, `lala_memory`

- [ ] **Collection Schemas**
  - [ ] `lala_chunks`: Vector size = embedding dim (384 for BGE-small), payload: `{document_id, chunk_index, chunk_text, score_placeholder}`
  - [ ] `lala_memory`: Same schema, payload includes `facts`, `capabilities`, `constraints`
  - [ ] Configure payload index on `document_id` and `chunk_index` for hybrid search

- [ ] **RagStore Refactor**
  - [ ] Create new `QdrantBackend` struct (parallel to SQLite)
  - [ ] Keep public API signature unchanged: `retrieve(query, k) → Vec<Chunk>`
  - [ ] Implement `retrieve()`:
    1. Call `/v1/embed` on query
    2. Vector search in Qdrant with top-k
    3. Fetch payload + embeddings
    4. Return Vec<Chunk> (same struct, score from Qdrant similarity)
  - [ ] Update `store()`:
    1. Chunk text (same algorithm)
    2. Call `/v1/embed` on each chunk
    3. Insert into Qdrant collection with embeddings + payload
    4. Also insert into database for audit (optional: keep SQLite for backup)

- [ ] **Deduplication**
  - [ ] Use Qdrant payload filter: `{source: url}` to prevent re-indexing
  - [ ] Example: Before inserting, query Qdrant for `has_key(payload, "source") && payload.source == target_url`

- [ ] **Testing**
  - [ ] Unit test: `retrieve(query, k)` returns correct type (Vec<Chunk>)
  - [ ] Integration test: Store chunk → embed → retrieve same query → get that chunk in top-k
  - [ ] Regression test: Verify old SQLite tests still pass (adapter pattern)

---

## 11. Performance Considerations

### SQLite FTS5 (Phase 0)

**Strengths:**
- Zero external dependencies (bundled)
- Fast keyword search for up to ~100K chunks
- BM25 scoring is semantically reasonable for keyword queries
- Low latency (microseconds per query on modern hardware)

**Limitations:**
- Keyword-only (no semantic similarity)
- Poor performance on synonymy ("document" vs "file")
- Large documents require manual keyword extraction
- No native filtering on metadata (e.g. "find chunks from source X")

### Qdrant (Phase 1)

**Advantages:**
- Semantic search (dense embeddings)
- Handles synonymy and paraphrasing
- Payload filtering (hybrid search: vector + metadata)
- Scales to millions of chunks
- Approximate Nearest Neighbor (ANN) indexing is efficient

**Trade-offs:**
- External service (docker dependency)
- Embedding inference latency (100–500ms depending on model)
- No "find chunks with exact phrase" (replaced by semantic similarity)
- Phase 1 migration requires embedding model in LLML

**Benchmarks (estimate):**
- FTS5 query (100K chunks): ~5–20ms, exact keywords
- Qdrant query (1M chunks): ~50–200ms, semantic similarity + filtering

---

## 12. Testing & Validation

### Unit Tests (Future)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_sliding_window() {
        let text = "A B C D E F G H";
        let chunks = chunk(text, 4, 1);
        assert_eq!(chunks.len(), 4);
        assert!(chunks[1].contains("C"));  // overlap check
    }

    #[test]
    fn test_chunk_empty() {
        let chunks = chunk("", 512, 64);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_store_and_retrieve() {
        let mut store = RagStore::open(":memory:").unwrap();
        store.store("Test", "/test", "The quick brown fox").unwrap();
        let results = store.retrieve("fox", 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].chunk_text, "The quick brown fox");
    }

    #[test]
    fn test_document_count() {
        let mut store = RagStore::open(":memory:").unwrap();
        store.store("Doc1", "/src1", "Content 1").unwrap();
        store.store("Doc2", "/src2", "Content 2").unwrap();
        assert_eq!(store.document_count().unwrap(), 2);
    }

    #[test]
    fn test_duplicate_source_skipped() {
        let mut store = RagStore::open(":memory:").unwrap();
        let (id1, _) = store.store("Doc", "/src", "Content A").unwrap();
        let (id2, _) = store.store("Doc", "/src", "Content B").unwrap();
        // Both calls return same doc_id; second content ignored
        assert_eq!(id1, id2);
    }
}
```

### Integration Tests

- **Chunking:** Verify overlap regions preserve context
- **Storage:** Insert docs, retrieve with various queries, check score ordering
- **Memory blocks:** Verify structure and placeholder extraction
- **News ingestion:** Mock RSS feed, verify parsing and dedup

---

## 13. Common Patterns

### Retrieving and Using Context

```rust
// In agent or CLI
let query = user_input;
let chunks = agent.retrieve_context(query)?;
let context_str = chunks
    .iter()
    .map(|c| c.chunk_text.as_str())
    .collect::<Vec<_>>()
    .join("\n---\n");

// Inject into system prompt
let system_with_context = format!(
    "{}\n\n--- Retrieved Context ---\n{}",
    SYSTEM_PROMPT, context_str
);

// Pass to model
let response = client.chat(&messages, &system_with_context)?;
```

### Ingesting a File

```rust
let file_path = "/path/to/document.md";
let content = std::fs::read_to_string(file_path)?;
let title = Path::new(file_path).file_name().unwrap().to_string_lossy();

let (doc_id, chunk_count) = store.store(&title, file_path, &content)?;
println!("Ingested {} as {} chunks (doc_id: {})", title, chunk_count, doc_id);
```

### Bulk Ingestion from Directory

```rust
for entry in std::fs::read_dir("/data")? {
    let path = entry?.path();
    if path.is_file() && path.extension().map(|e| e == "md").unwrap_or(false) {
        let content = std::fs::read_to_string(&path)?;
        let title = path.file_name().unwrap().to_string_lossy();
        match store.store(&title, path.to_str().unwrap(), &content) {
            Ok((_, chunks)) => println!("✓ {} ({} chunks)", title, chunks),
            Err(e) => println!("✗ {}: {}", title, e),
        }
    }
}
```

---

## 14. Troubleshooting

### No Results from `retrieve(query, k)`

**Cause:** Query keywords don't match any chunks.

**Debug:**
1. Check that documents were stored: `store.document_count()`
2. Try simpler query: `store.retrieve("the", 5)` (should match almost anything)
3. Check `chunks_fts` table directly: `SELECT COUNT(*) FROM chunks_fts`

### Memory Blocks Returning Placeholder Values

**Expected in Phase 0:** All three fields (facts, capabilities, constraints) are equal to chunk_text.

**Phase 1 fix:** Wait for LLM extraction or manually call `update_memory_block()`.

### Duplicate Documents Stored

**Cause:** `source` field was different for same file.

**Fix:** Normalize paths before calling `store()`:
```rust
let abs_path = std::fs::canonicalize(path)?;
store.store(title, abs_path.to_str().unwrap(), content)?;
```

### HTTP 403 on RSS Feed Items

  **Expected:** Function retries via Allorigins proxy.

**If still failing:** Allorigins may be rate-limited or blocked. Fallback:
- Use `DELAY_MS` parameter to increase delay between fetches
- Manually fetch and store articles via file input

---

## References

- **SQLite FTS5:** https://www.sqlite.org/fts5.html
- **Qdrant Docs:** https://qdrant.tech/documentation/
- **BGE Embeddings:** https://github.com/FlagOpen/FlagEmbedding
- **Phase 0 Design:** [doc/planning/phase0-rag.md](planning/phase0-rag.md)
