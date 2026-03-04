-- Enable vector support
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- 1️⃣ Documents table
CREATE TABLE documents (
    id BIGSERIAL PRIMARY KEY,
    title TEXT,
    source TEXT,
    url TEXT,
    published_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW()
);

-- 2️⃣ Document chunks (RAG embeddings)
CREATE TABLE document_chunks (
    id BIGSERIAL PRIMARY KEY ,
    document_id BIGINT REFERENCES documents(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    embedding VECTOR(384),
    embedding_model TEXT,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Add index for fast similarity search
CREATE INDEX document_chunks_embedding_idx
ON document_chunks
USING ivfflat (embedding vector_cosine_ops)
WITH (lists = 100);

-- 3️⃣ Agent memory (long-term)
CREATE TABLE memory (
    id BIGSERIAL PRIMARY KEY,
    content TEXT NOT NULL,
    embedding VECTOR(384),
    embedding_model TEXT,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Optional: analyze for performance
ANALYZE document_chunks;
ANALYZE memory;