FROM postgres:18

# Install pgvector dependencies
RUN apt-get update && \
    apt-get install -y postgresql-server-dev-18 build-essential git && \
    rm -rf /var/lib/apt/lists/*

# Clone and install pgvector
RUN git clone https://github.com/pgvector/pgvector.git /pgvector && \
    cd /pgvector && \
    make && make install && \
    cd / && rm -rf /pgvector

# Optional: copy init.sql
COPY init.sql /docker-entrypoint-initdb.d/init.sql