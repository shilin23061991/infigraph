# Recipe: Infigraph E2E Team Setup (AI Foundation ~30 repos)

**Goal:** Install infigraph in a shared environment, ingest ~30 repos into a combined knowledge graph, expose query APIs, and auto-update on PR merge.

**Presentation target:** Wed 2026-07-22, AI Foundation team-wide.

---

## Phase 1: Install on E2E Machine

### Option A: One-liner (macOS/Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/intuit/infigraph/main/install.sh | bash
```

This clones, builds `infigraph-cli` + `infigraph-mcp`, copies to `~/.local/bin`, and runs `infigraph install` (configures Claude Code MCP).

### Option B: Manual build

```bash
# Prerequisites: Rust (rustup), cmake
git clone https://github.com/intuit/infigraph.git
cd infigraph
cargo build --release -p infigraph-cli -p infigraph-mcp

# Copy binaries
cp target/release/infigraph target/release/infigraph-mcp ~/.local/bin/

# Register MCP with Claude Code
infigraph install
```

### Option C: cargo install

```bash
cargo install infigraph-cli infigraph-mcp
infigraph install
```

### Verify

```bash
infigraph --version
infigraph-mcp --version
```

---

## Phase 2: Ingest ~30 Repos

### 2a. Clone all repos (shallow)

```bash
# Create fleet directory
mkdir -p ~/fleet && cd ~/fleet

# Clone from your repo list (example using eng_pulse ALL_REPOS or a repos.txt)
while read repo; do
  name=$(basename "$repo" .git)
  [ -d "$name" ] || git clone --depth 1 "$repo" "$name"
done < repos.txt
```

`repos.txt` format — one GitHub URL per line:
```
https://github.com/AIF-TW/repo-1.git
https://github.com/AIF-TW/repo-2.git
...
```

### 2b. Create group and add repos

```bash
# Create the group
infigraph group create aif-fleet

# Add all repos
for dir in ~/fleet/*/; do
  infigraph group add aif-fleet "$dir"
done
```

### 2c. Index everything (full build)

```bash
# This runs: per-repo index → sync contracts → detect deps → link edges → build combined graph + docs
infigraph group build aif-fleet
```

**Expect ~15-20 min for 30 repos** (794s measured for 31-repo fleet on M-series Mac). Cross-service linking phase is CPU-bound and may appear silent — that's normal.

### 2d. Verify

```bash
# Check stats
infigraph group query aif-fleet --stats

# Test search
infigraph group search aif-fleet "authentication handler"

# Test Cypher (code graph)
infigraph group query aif-fleet "MATCH (n:Symbol) RETURN n.kind, count(*) ORDER BY count(*) DESC LIMIT 10"
```

---

## Phase 3: Query APIs

### Currently available: MCP (Claude Code / IDE)

```bash
# Already configured by `infigraph install`
# In Claude Code, infigraph tools are available:
#   search, get_doc_context, trace_callers, group_search, group_query, etc.
```

### REST API — NOT YET BUILT

> **Gap:** There is no `infigraph serve` HTTP endpoint today. The MCP server uses stdio transport (designed for IDE/Claude Code integration, not HTTP clients).
>
> **Options for the presentation:**
>
> 1. **Demo via CLI** — `infigraph group search` / `infigraph group query` work from terminal
> 2. **Demo via Claude Code** — show MCP tools in action (search, trace_callers, get_architecture)
> 3. **Wrap CLI in a thin API** — quick script:

```bash
#!/bin/bash
# thin-api.sh — minimal HTTP wrapper (demo only, not production)
# Requires: socat or python
python3 -c "
from http.server import HTTPServer, BaseHTTPRequestHandler
import subprocess, json, urllib.parse

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        params = urllib.parse.parse_qs(parsed.query)
        q = params.get('q', [''])[0]
        if parsed.path == '/search':
            result = subprocess.run(
                ['infigraph', 'group', 'search', 'aif-fleet', q],
                capture_output=True, text=True
            )
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain')
            self.end_headers()
            self.wfile.write(result.stdout.encode())
        elif parsed.path == '/query':
            result = subprocess.run(
                ['infigraph', 'group', 'query', 'aif-fleet', q],
                capture_output=True, text=True
            )
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain')
            self.end_headers()
            self.wfile.write(result.stdout.encode())
        else:
            self.send_response(404)
            self.end_headers()

HTTPServer(('0.0.0.0', 8642), Handler).serve_forever()
"
```

```bash
# Usage:
curl "http://localhost:8642/search?q=authentication+handler"
curl "http://localhost:8642/query?q=MATCH+(n:Symbol)+RETURN+n.kind,+count(*)+LIMIT+10"
```

---

## Phase 4: Auto-Update on PR Merge (GitHub Action)

> **Gap:** No official GitHub Action exists yet. Below is a working recipe.

### `.github/workflows/infigraph-update.yml`

```yaml
name: Update Infigraph Knowledge Graph

on:
  push:
    branches: [main, master]  # triggers on merge to default branch

jobs:
  update-graph:
    runs-on: ubuntu-latest  # or self-hosted runner with persistent storage
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # full history for better analysis

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install cmake
        run: sudo apt-get install -y cmake

      - name: Cache infigraph binary
        uses: actions/cache@v4
        with:
          path: ~/.cargo/bin/infigraph
          key: infigraph-${{ runner.os }}-${{ hashFiles('Cargo.lock') }}

      - name: Install infigraph
        run: |
          if ! command -v infigraph &>/dev/null; then
            cargo install infigraph-cli
          fi

      - name: Index this repo
        run: infigraph index .

      - name: Upload graph artifact
        uses: actions/upload-artifact@v4
        with:
          name: infigraph-graph
          path: .infigraph/
          retention-days: 30
```

### For fleet-wide rebuild (centralized runner)

```yaml
name: Rebuild Fleet Knowledge Graph

on:
  workflow_dispatch:  # manual trigger
  schedule:
    - cron: '0 2 * * *'  # nightly at 2am

jobs:
  rebuild:
    runs-on: [self-hosted, infigraph]  # needs persistent disk
    steps:
      - name: Install infigraph
        run: cargo install infigraph-cli || true

      - name: Pull all repos
        run: |
          cd ~/fleet
          for dir in */; do
            (cd "$dir" && git pull --ff-only) || true
          done

      - name: Rebuild group
        run: infigraph group build aif-fleet

      - name: Report stats
        run: infigraph group query aif-fleet --stats
```

### Migrating from Jenkins

If current Jenkins pipeline does something like:
```groovy
// Jenkins
pipeline {
  triggers { pollSCM('H/5 * * * *') }
  stages {
    stage('Build') { steps { sh 'make index' } }
  }
}
```

Replace with the GHA above. Key differences:
- **Trigger:** `on: push` (webhook) replaces `pollSCM` — faster, no wasted cycles
- **Caching:** GHA cache action replaces workspace persistence
- **Artifact:** upload `.infigraph/` dir instead of archiving build outputs

---

## What to Show in Presentation

| Demo | Command / Tool | Impact |
|------|---------------|--------|
| Cross-repo search | `infigraph group search aif-fleet "payment processing"` | Find code across 30 repos instantly |
| Architecture map | `infigraph get_architecture` (via MCP) | Auto-generated component diagram |
| Blast radius | `infigraph trace_callers <symbol>` → `transitive_impact` | "If I change X, what breaks?" |
| Contract detection | `infigraph group query aif-fleet --stats` | Shows cross-service contracts |
| Cypher queries | `MATCH (s:Symbol)-[:CALLS]->(t:Symbol) WHERE s.repo <> t.repo RETURN ...` | Cross-repo dependencies |
| Doc search | `infigraph group search aif-fleet "deployment guide" --scope docs` | Unified doc search |

---

## Remote Mode (Neo4j + Postgres)

For deployments needing concurrent writes and shared storage, use remote mode with Neo4j + pgvector sidecars. See [REMOTE-MULTI-REPO.md](REMOTE-MULTI-REPO.md) for full setup.

```bash
# Build with remote features
cargo install infigraph-cli --features remote
cargo install infigraph-mcp --features remote

# Start sidecars (Docker)
docker run -d --name neo4j -p 7687:7687 -e NEO4J_AUTH=neo4j/infigraph neo4j:5-community
docker run -d --name postgres -p 5432:5432 \
  -e POSTGRES_USER=infigraph -e POSTGRES_PASSWORD=infigraph \
  -e POSTGRES_DB=infigraph pgvector/pgvector:pg16

# Set env and index
export INFIGRAPH_BACKEND=neo4j
export NEO4J_URI=127.0.0.1:7687 NEO4J_USER=neo4j NEO4J_PASSWORD=infigraph
export DATABASE_URL="host=localhost user=infigraph password=infigraph dbname=infigraph"

infigraph group build aif-fleet  # parallel indexing via Neo4j
```

**Search in remote mode** uses Neo4j for BM25 symbol index and pgvector for vector embeddings. HNSW is skipped (brute-force is faster under 200K symbols).

---

## Known Gaps for Production

| Gap | Status | Workaround |
|-----|--------|------------|
| REST/HTTP API | Not built | CLI wrapper script above, or MCP via Claude Code |
| Official GitHub Action | Not published | YAML recipe above works |
| Concurrent writes | Kuzu single-writer | Use remote mode (Neo4j) for concurrent writes |
| Symbol embeddings (remote) | Write path missing | Search works (BM25), vector ranking not yet active |
| Auth/ACL | Not built | Restrict runner access; single-tenant for now |
| Pre-built binaries | GitHub releases | `cargo install` from source (~3 min) |

---

## Timeline to Wed 7/22

| Day | Task |
|-----|------|
| Mon 7/20 | Install on e2e machine, clone repos, run `group build` |
| Mon 7/20 | Verify search + Cypher queries work |
| Tue 7/21 | Set up GHA workflow on one repo, test trigger |
| Tue 7/21 | Dry-run presentation demos |
| Wed 7/22 | Present |
