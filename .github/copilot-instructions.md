# lala — Agentic RAG System

Rust-based local **Agentic RAG** (Retrieval-Augmented Generation) system. Runs an LLM locally via `llama_cpp` and uses PostgreSQL + pgvector for document/memory storage and semantic retrieval.

## Build & Run

```sh
# Build
cd lala && cargo build

# Run (model path required — place GGUF files in model/)
cd lala && cargo run -- model/<your-model>.gguf

# Database (PostgreSQL + pgvector)
docker build -f psql.Dockerfile -t lala-postgres .
docker run -e POSTGRES_PASSWORD=postgres -p 5432:5432 lala-postgres
```

`DATABASE_URL` environment variable must be set for DB operations, e.g.:
```
DATABASE_URL=postgres://postgres:postgres@localhost:5432/lala
```

## Architecture

```
lala/src/
  main.rs          # Entry point — reads model path from argv
  cli.rs           # Interactive REPL loop, prompt building, token streaming
  agent/
    model.rs       # ModelWrapper (load GGUF) + SessionWrapper (llama_cpp session)
  db/
    connection.rs  # PgPool init, document_chunks CRUD, memory CRUD
```

The full agentic pipeline design (query rewriting, retrieval planning, reranking, grounding) is documented in [doc/design.md](../doc/design.md). See also [doc/queries.md](../doc/queries.md) and [doc/retrival.md](../doc/retrival.md).

## Key Conventions

- **Error handling**: propagate with `anyhow::Result`; avoid `.unwrap()` in new code except DB internals already using it.
- **Embeddings**: represented as `Vec<f32>`, stored in pgvector columns. Embedding model is `"bge-small"` — keep consistent with existing DB rows.
- **Similarity search**: uses pgvector `<=>` operator (cosine distance) in SQL queries.
- **LLM prompt format**: Mistral/Llama `[INST]...[/INST]` — see `cli.rs:build_prompt`.
- **GGUF models**: store in `model/` (gitignored via `.keep`).
- **Async DB code**: all `sqlx` calls are `async`; wire into `tokio` runtime when integrating with the sync CLI loop.

## Database Schema

Tables: `document_chunks`, `memory`. Full ER diagram and field descriptions in [doc/design.md](../doc/design.md).  
`init.sql` (not yet created) should be placed in the repo root and will be automatically run by the Docker container on first start.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `llama_cpp` | Local LLM inference (GGUF format) |
| `sqlx` | Async PostgreSQL via `PgPool` |
| `anyhow` | Error propagation |
| `rustyline` | Readline-style CLI input (not yet wired in) |

`llama_cpp` requires C++ build toolchain — `build.rs` exists for this purpose.
