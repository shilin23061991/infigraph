# Intuit-Wide Knowledge Graph — Design Document

## Problem

Intuit has thousands of repos across many BUs and teams. Engineers need to understand cross-repo dependencies, trace API contracts across services, find who calls what, and reason about blast radius — across the entire org, not just one repo at a time.

Infigraph already solves this locally: it indexes repos into a graph (kuzu), runs cross-repo linking via groups, and supports vector search (usearch). But the current architecture is **embedded, local, single-writer, file-watch-driven**. Every one of those breaks at org scale.

This doc covers: the **combined Intuit-wide graph** (org → BU → team → repo), storage, **GitHub merge → default-branch sync**, database choices, and the federation model. **Local laptop mode is unchanged** — online is a separate serving path.

---

## Product Modes (Local Untouched)

| Mode | Store | Who uses it |
|------|--------|-------------|
| **Local** | Per-repo `.infigraph/` Kuzu on the laptop | Solo / offline / existing MCP today |
| **Hosted (org-wide)** | Central graph + vector + Postgres | Team / BU / org queries; **no local DB required** for consumers |

Indexers in CI may still use ephemeral Kuzu to build an export artifact. That is build-time only — not the serving store.

MCP routing:

```
scope=local | group  → existing laptop path (unchanged)
scope=team | bu | org → Query Service API (hosted)
```

---

## Current Stack (What Exists Today)

| Layer | Technology | Limitation at Scale |
|-------|-----------|-------------------|
| Graph DB | kuzu (lbug) — embedded columnar graph | Single-writer, single-process, local files |
| Relational/logic | cozo + SQLite | Per-user local storage |
| Vector search | usearch (HNSW) | Local index files, no shared serving |
| Embeddings | model2vec-rs | Local inference, fast but per-machine |
| Columnar export | parquet + arrow | File-based, no query serving |
| File watching | notify (kqueue/inotify) | Local filesystem only |
| Cross-repo | `group_create/add/index/sync/link` | Manual, requires all repos cloned locally |
| Storage path | `~/.infigraph/` per user | No sharing between users or CI |

**What works well and should be preserved:**
- Tree-sitter parsing → graph construction pipeline
- SCIP import for LSP-grade precision
- Cross-repo contract detection (`group_sync`, `group_link`)
- Hybrid search (BM25 + vector + grep)
- Incremental reindex via `detect_changes`
- Existing code schema (`Symbol`, `CALLS`, `CALLS_SERVICE`, …) as the code-intelligence core

---

## Architecture (Recommended)

Embedded DBs (kuzu, cozo, usearch) are great for local dev and CI build steps. They cannot serve org-wide queries. The org-wide system needs **server-grade** storage for both graph and vectors.

```
┌──────────────────────────────────────────────────────────────┐
│              GitHub (default branch only)                     │
│  PR merge / push to default_branch → webhook or GHA          │
└──────────────────────┬───────────────────────────────────────┘
                       │
┌──────────────────────▼───────────────────────────────────────┐
│                     CI / Build Layer                          │
│  clone @ default_branch @ commit_sha                         │
│  infigraph ci-export (tree-sitter + SCIP)                    │
│  → nodes, edges, embeddings, contracts                       │
│  → upload artifact → ingest API                              │
└──────────────────────┬───────────────────────────────────────┘
                       │ writes
┌──────────────────────▼───────────────────────────────────────┐
│                  Central Data Stores                          │
│                                                               │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────┐  │
│  │  Graph DB    │  │  Vector DB   │  │  Search / BM25     │  │
│  │  (Memgraph   │  │  (Qdrant)    │  │  (optional         │  │
│  │   or Neptune)│  │              │  │   OpenSearch)      │  │
│  └──────────────┘  └──────────────┘  └────────────────────┘  │
│                                                               │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │  Metadata Store (PostgreSQL)                             │ │
│  │  org/BU/team/repo registry, default_branch, ACL,         │ │
│  │  contracts, link registry, job audit                     │ │
│  └──────────────────────────────────────────────────────────┘ │
└──────────────────────┬───────────────────────────────────────┘
                       │ reads
┌──────────────────────▼───────────────────────────────────────┐
│                  Query / API Layer                            │
│  ┌─────────┐  ┌──────────────┐  ┌──────────────────────┐     │
│  │ MCP     │  │ REST / gRPC  │  │ Web Dashboard        │     │
│  │ Server  │  │ API          │  │ (dep explorer, etc.) │     │
│  └─────────┘  └──────────────┘  └──────────────────────┘     │
└──────────────────────────────────────────────────────────────┘
```

**Key shift:** kuzu/cozo/usearch stay as the **build-time** tool in CI. They produce nodes, edges, and embeddings. Those get pushed into centralized stores. Consumers never open a laptop DB for org scope.

---

## Combined Intuit-Wide Graph

### Org hierarchy

```
Org (Intuit)
 └─ BU          (e.g. CG, PTG, SBG, IES, …)
     └─ Team
         └─ Repo(s)
```

**One logical graph for the company** — not a separate database per BU. Cross-team and cross-BU dependencies are normal edges. Hierarchy is for **ownership, ACL, and query scope**, not physical isolation.

Exception (later): compliance air-gap → shard by BU + thin federation for approved shared packages. Do not start there.

### What stays from today's code schema

Reuse the existing code-intelligence model from `crates/infigraph-core/src/graph/schema.rs`:

**Nodes:** `Symbol`, `Module`, `File`, `Folder`, `Cluster`, `Dependency`, `Statement`, `Concern`, `ConfigBinding`, …

**Edges:** `CALLS`, `IMPORTS`, `CONTAINS`, `INHERITS`, `TESTED_BY`, `READS`, `WRITES`, `DEFINES`, `DEPENDS_ON`, `CALLS_SERVICE`, `BRIDGE_TO`, …

Local indexing continues to produce this shape. Online ingest **stamps multi-repo identity** on top — it does not invent a parallel code model.

### Additions for the combined org graph

| Addition | Layer | Purpose |
|----------|--------|---------|
| `Repo` node | Graph | Anchor per repository |
| `repo_slug` on Symbol / File / Module / … | Graph props | Filter and delete-by-repo without always joining |
| Globally unique IDs | Ingest rewrite | `repo_slug::file::symbol` — avoids collisions across repos |
| `IN_REPO` | Edge | Symbol/File/Module → Repo; enables replace-subgraph-on-reindex |
| Optional `Org` / `BU` / `Team` nodes | Graph (thin) | Cypher convenience: `MATCH (t:Team)-[:OWNS]->(r:Repo)` |
| Org → BU → Team → Repo | **Postgres registry (source of truth)** | Owners, ACL, GitHub mapping, `default_branch` |

**Do not** put HR trees, tickets, or people graphs into the code store. Ownership lives in the registry; optionally mirrored as thin graph nodes for traversal UX.

### Data model (hosted)

```
# Ownership (registry primary; optional thin graph mirror)
(:Org {id, name})
(:BU {id, name, org_id})
(:Team {id, name, bu_id})
(:Repo {
  slug,           # e.g. "intuit/payments-api"
  url,
  default_branch, # resolved — see Default Branch section
  last_indexed_commit,
  bu, team, org,
  visibility
})

(:Org)-[:HAS_BU]->(:BU)-[:HAS_TEAM]->(:Team)-[:OWNS]->(:Repo)

# Code (existing schema + multi-repo stamps)
(:Symbol {
  id,             # "repo_slug::path::name" online
  name, kind, file, start_line, end_line,
  language, visibility, docstring, category, …
  repo_slug, bu, team
})
(:File { id, path, language, repo_slug, … })
(:Module { id, name, file, language, repo_slug, … })

(:Symbol)-[:IN_REPO]->(:Repo)
(:File)-[:IN_REPO]->(:Repo)
(:Module)-[:IN_REPO]->(:Repo)

# Same code edges as local (targets are globally unique IDs)
(:Symbol)-[:CALLS]->(:Symbol)
(:Module)-[:IMPORTS]->(:Module)
(:Module)-[:CONTAINS]->(:Symbol)
(:Symbol)-[:INHERITS]->(:Symbol)
(:File)-[:DEFINES]->(:Symbol)
(:Symbol)-[:CALLS_SERVICE {method, path, target_service}]->(:Symbol)
(:Repo)-[:DEPENDS_ON]->(:Repo)   # derived from contracts / package / service links
```

### Scaling implications (summary)

| Pressure | Approach |
|----------|----------|
| Thousands of repos | Index unit = **one repo** (or virtual-repo) per CI job |
| Graph RAM | Plan **128–256GB** Memgraph for ~5k repos (properties!); ceiling contingency required |
| Write storms | Full replace for normal repos; **patch ingest for monorepos** |
| Vectors | Few Qdrant collections + payload filters — not 5k collections |
| Cross-repo links | Registry dependents; **lazy/eventual** for platform packages |
| Query cost | Default `team`/`bu`; hop caps; ACL pushdown |
| Cost | **~$5–12k/mo** realistic for ~5k repos — not $1.2k |

Full analysis: [Scaling Challenges](#scaling-challenges--mitigations), [Phase 0](#phase-0-decisions-acl--topology).

### Query scopes

| Scope | Meaning |
|-------|---------|
| `local` | Current repo `.infigraph/` (today — unchanged) |
| `group` | Manual multi-repo group (today — unchanged) |
| `team` | All repos owned by the caller's team |
| `bu` | All repos in the BU (cross-BU edges still visible when following deps) |
| `org` | Full company graph |

---

## Storage Design

### What Gets Stored

Each repo index (at a default-branch commit) produces:

| Data | Volume (per repo, 10k symbols) | Destination |
|------|-------------------------------|-------------|
| Graph nodes (Symbol, Module, File, …) | ~10k rows | Graph DB |
| Graph edges (CALLS, CONTAINS, …) | ~25k rows | Graph DB |
| Cross-repo edges (contracts, CALLS_SERVICE, Repo DEPENDS_ON) | ~100–1k rows | Graph DB |
| Embeddings (symbol + doc vectors, 256-dim) | ~10k vectors, ~10MB | Vector DB |
| Full-text tokens | ~5MB | Search / Qdrant sparse |
| Metadata (repo, commit, **default_branch**, languages, bu, team) | ~1KB | PostgreSQL |
| Contracts (exported API surface) | ~50KB JSON | PostgreSQL |

**Total for 5,000 repos:** ~50M graph nodes, ~125M edges, ~50M vectors (~50GB), ~25GB text index.

Hot store keeps **latest indexed default-branch commit only**. Older exports archive to object storage for audit.

---

## Database Choices

### Graph DB — for traversal queries (callers, callees, blast radius, dependency chains)

| Option | Type | Strengths | Weaknesses |
|--------|------|-----------|------------|
| **Amazon Neptune** | Managed, Gremlin/openCypher | Zero-ops, AWS-native, scales reads via replicas | Expensive, write throughput limited, vendor lock-in |
| **Memgraph** | In-memory, Cypher | Fastest traversals, real-time updates, MAGE algorithms | RAM ∝ graph size; self-hosted |
| **TigerGraph** | Distributed, GSQL | Massive scale | Heavy setup, GSQL |
| **Neo4j AuraDB** | Managed, Cypher | Mature ecosystem | Cost / licensing at scale |
| **Apache AGE (PostgreSQL)** | Extension | Reuses PG | Weaker deep traversals |

**Recommendation: Memgraph** (self-hosted on k8s) or **Neptune** (if AWS-managed preferred).

### Vector DB — for semantic search

**Current (pod-local):** Postgres + pgvector sidecar. Embeddings stored in `embeddings` table with `vector(256)` column, filtered by `kind` (`symbol`, `doc_chunk`). Brute-force cosine scoring in-memory (materialized via `all_embeddings(kind)`). Sufficient for <200K symbols per deployment.

**Future (org-scale):** Qdrant. Prefer **a small number of collections** (e.g. one per BU, or one global + shard by payload `repo_slug`) with payload filters for `repo_slug` / `bu` / `team` / `language` / `symbol_kind`. Avoid one collection per repo at 5k+ repos (control-plane and file-handle blowups).

### Full-Text Search

**Recommendation:** Start with **Qdrant hybrid** (dense + sparse). Add OpenSearch only if regex / analyzer needs demand it. Current pod-local mode uses in-memory BM25 built from Neo4j symbol rows.

### Metadata Store — PostgreSQL

Source of truth for:
- Org / BU / Team / Repo registry
- **`default_branch`** per repo (resolved + optional override)
- Contract manifests and cross-repo link registry
- Index job audit log
- ACL / access control

### What Stays Embedded (Build-Time Only)

| Tool | Role |
|------|------|
| **kuzu (lbug)** | CI: parse → local graph → export nodes/edges |
| **usearch** | CI: embeddings → push to Qdrant |
| **cozo** | CI: contract extraction → Postgres |
| **tree-sitter / SCIP / model2vec-rs** | CI: parse / precise / embed |

These never serve org queries directly.

### Not Recommended for This Use Case

| DB | Why Not |
|----|---------|
| **kuzu/cozo as serving layer** | Embedded, single-writer |
| **One graph DB per BU** | Breaks cross-BU edges (unless compliance forces it later) |
| **MongoDB / RedisGraph** | Poor traversal / EOL |
| **DuckDB as serving DB** | Analytics only |

---

## Sync: Graph Updates on GitHub Merge (Default Branch)

The hosted graph reflects **default-branch HEAD only** — the code teams ship after merge. Feature branches and open PRs are out of scope for v1 (optional later: PR preview indexes).

### Default branch resolution

Do **not** hardcode `main` or `master`. Many Intuit repos use either, or a custom name (`develop`, `release`, etc.).

**Resolution order** (first match wins, then persist in Postgres registry):

1. **Platform override (user/admin defined)**  
   Registry field `repos.default_branch_override` set via admin API, onboarding config, or `.infigraph/org.toml` / equivalent.  
   Use when GitHub's default is wrong for indexing (e.g. GitHub default is `main` but production ship branch is `release`).

2. **GitHub repository default**  
   `GET /repos/{owner}/{repo}` → `default_branch`.  
   This is GitHub's configured default (whatever the repo owner set — often `main` or `master`).

3. **Fallback (onboarding only)**  
   If the API is unreachable during first register: try `main`, then `master`, record uncertainty, and require explicit override before scheduling recurring jobs.

On every successful index job, refresh `default_branch` from GitHub unless an override is set (so renames of the GitHub default are picked up automatically).

```
registry.repos
  slug
  default_branch              # effective branch used for clone + webhook filter
  default_branch_override     # NULL = use GitHub; non-NULL = force this name
  default_branch_source       # 'override' | 'github' | 'fallback'
  last_indexed_commit
  last_indexed_at
  bu, team, org
```

### What triggers an update

| Event | Action |
|-------|--------|
| **`push` to the repo's effective default branch** | Enqueue index job for `after` commit SHA |
| **PR merged** into default branch | Appears as a `push` to that branch — same path (no separate PR handler required) |
| **`push` to any other branch** | **Ignore** |
| **PR opened / synchronize** | **Ignore** (v1) |
| **`repository.edited`** (default branch renamed) | Refresh registry `default_branch`; do not reindex until next push |
| **Manual / admin reindex** | Enqueue job for current default-branch HEAD |
| **First onboarding of a repo** | Full index of default-branch HEAD |

Prefer **`push` webhooks** filtered by branch name over `pull_request.closed`, so direct pushes to default (hotfix, admin) are covered.

### End-to-end flow

```
1. Developer merges PR (or pushes) to default branch
2. GitHub sends push webhook
3. Sync service:
     a. Load repo from registry
     b. Resolve effective default_branch (override || GitHub)
     c. If webhook.ref != refs/heads/<default_branch> → drop
     d. Dedupe by (repo_slug, commit_sha); enqueue otherwise
4. Build worker:
     a. git clone --depth=N --branch=<default_branch> @ commit_sha
     b. infigraph ci-export → artifact (nodes/edges/vectors/contracts)
     c. Upload artifact to object storage
     d. POST ingest API
5. Ingestion:
     a. Transaction: delete prior subgraph WHERE repo_slug = …
     b. Insert new nodes/edges (IDs rewritten with repo_slug:: prefix)
     c. Upsert Repo node + last_indexed_commit
     d. Upsert vectors in Qdrant for that repo
     e. Write contracts + job row to Postgres
6. Link service (async):
     a. Diff contracts vs previous
     b. Re-link only registered dependents / dependencies
     c. Update cross-repo CALLS_SERVICE / Repo DEPENDS_ON edges
```

```
GitHub push (default branch)
        │
        ▼
┌───────────────────┐
│  Sync / webhook   │  filter by effective default_branch
└─────────┬─────────┘
          │ enqueue (repo, sha, branch)
          ▼
┌───────────────────┐
│  CI / build job   │  clone branch @ sha → ci-export → S3
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│  Ingest service   │  replace repo subgraph + vectors + meta
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│  Link service     │  refresh cross-repo / cross-BU edges
└───────────────────┘
```

### GitHub Actions shape (per repo or org workflow)

```yaml
# Conceptual — runs only on the repo's default branch
on:
  push:
    branches:
      - main      # placeholder; prefer workflow that reads default_branch
                  # or org-level dispatcher triggered by central webhook

jobs:
  infigraph-index:
    # Only meaningful when github.ref_name == registry.default_branch
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.sha }}
      - run: infigraph ci-export --output artifact/
      - run: |
          curl -X POST "$INGEST_URL/v1/ingest" \
            -H "Authorization: Bearer $TOKEN" \
            -F "repo=$GITHUB_REPOSITORY" \
            -F "commit=$GITHUB_SHA" \
            -F "branch=${{ github.ref_name }}" \
            -F "artifact=@artifact.tgz"
```

**Org-scale preference:** one central webhook + build queue (not thousands of copy-pasted workflows), still cloning each repo at its own `default_branch`.

### Incremental reindex

Full reindex on every merge is wasteful. Reuse `detect_changes`:

1. Read previous `last_indexed_commit` from registry.
2. `git diff --name-only $LAST..$NEW` → changed files.
3. Re-parse only those files in the CI ephemeral graph.
4. **Ingest mode by repo class:**
   - **Normal repos** (&lt; ~50k symbols): export + **full subgraph replace** (simple, consistent).
   - **Large / monorepo** (≥ threshold or flagged `ingest_mode=patch`): export a **patch artifact** (delete/upsert only changed file paths + prune deleted paths). Full replace is an availability event at 500k–2M symbols and is **not** acceptable as the steady-state path.

**Expected build time:** &lt;30s incremental (typical merge); monorepo patch ingest measured in tens of seconds of graph mutations, not multi-minute full wipe/reload.

### Cross-repo / cross-BU link refresh

When repo A updates on its default branch:

1. Link service reads A's new contracts.
2. Diffs against previous contracts — **if public surface unchanged, skip**.
3. Classify A:
   - **Normal provider:** re-link registered dependents B where B→A (coalesce bursts; rate-limit).
   - **Platform / high-fan-out provider** (dependents above threshold, e.g. auth SDK): **do not** eagerly re-link thousands of B's. Mark contracts dirty; use **lazy link-on-read** or eventual batch with priority queue. Staleness is acceptable within a published SLO (e.g. p95 &lt; 1h for platform edges).
4. Update cross-repo edges in the org graph (including cross-team / cross-BU).

Isolated repos (no contracts) remain standalone subgraphs — still queryable by `repo_slug` / team / bu.

### Idempotency and ordering

- Key jobs by `(repo_slug, commit_sha)` — duplicate webhooks are no-ops.
- If commit B is queued while commit A (older) is still running, **prefer newest SHA** for that repo (cancel or skip stale jobs).
- Concurrent merges in different repos: independent subgraph replaces; link service serializes per edge-pair or uses optimistic retries.

### What is intentionally not indexed (v1)

- Non-default branches  
- Open PR heads  
- Tags / releases (unless they move default)  
- Historical commits in the hot graph (archive only)

---

## Query Model

### MCP Interface

| Tool | Local (today) | Org-wide (new) |
|------|--------------|----------------|
| `search` | Indexed local repos | `scope=team\|bu\|org` → hosted |
| `trace_callers` | Within repo/group | Follows cross-repo edges in central graph |
| `get_dependencies` | Local deps | Includes cross-repo / cross-BU `DEPENDS_ON` |
| `group_query` | Manual groups | Optional: registry-backed auto teams |

### Query Routing

```
MCP Request
  ├─ scope=local  → existing local kuzu (unchanged)
  ├─ scope=group  → existing group logic (unchanged)
  ├─ scope=team   → Query Service (filter repo.team)
  ├─ scope=bu     → Query Service (filter repo.bu)
  └─ scope=org    → Query Service (full graph, ACL-filtered)
                      ├─ graph traversal  → Memgraph / Neptune
                      ├─ semantic search  → Qdrant
                      └─ metadata         → PostgreSQL
```

---

## Deployment

### Components

| Component | Runs Where | Scaling |
|-----------|-----------|---------|
| **Build workers** | CI (GitHub Actions / Jenkins) / k8s jobs | One job per default-branch update |
| **Memgraph / Neptune** | k8s or managed | Read replicas for query scale |
| **Qdrant** | k8s | Sharded by repo |
| **PostgreSQL** | RDS / k8s | Registry + ACL + jobs |
| **Query service** | k8s | Stateless |
| **Webhook / sync service** | k8s or Lambda | Filter by default_branch, enqueue |
| **Ingestion + link services** | k8s | Replace subgraph; async re-link |

### Rollout Phases

**Phase 0 — Decisions that block architecture (before build)**
- **ACL model** (see [Phase 0 decisions](#phase-0-decisions-acl--topology)): GitHub-mirror vs BU-scoped vs org-wide read. This is not optional — Intuit regulatory posture likely forbids org-wide symbol/contract visibility.
- **Topology**: start single-logical-graph **or** BU-federated + contract registry (see tradeoff below). Pick with measured query mix if possible; default remains single graph with an explicit Memgraph-ceiling contingency.
- **Cost / capacity envelope** approved with realistic numbers ($5–12k/mo for ~5k repos), not the early $1.2k sketch.

**Phase 1 — CI Indexer + Central Graph**
- `infigraph ci-export`; ingest to Memgraph + Qdrant + Postgres.
- Webhook / GHA on **effective default branch** only.
- Registry stores `default_branch` + optional override.
- **Monorepo patch ingest** required for flagged large repos (not deferred).
- Ingest **backpressure**: queue depth limits, circuit breaker when graph primary &gt; RAM/CPU SLO (shed or delay non-priority jobs; never unbounded queue).

**Phase 2 — Query Service + MCP `scope=`**
- `team` / `bu` / `org` scopes; local/group unchanged.
- ACL pushdown on every query.

**Phase 3 — Cross-Repo Linking (server-side)**
- Port `group_sync` / `group_link`; cross-BU edges.
- **Platform-package lazy / eventual link** for high-fan-out providers.

**Phase 4 — Analytics + Dashboard**
- Dep explorer, blast radius, dead APIs, MAGE analytics.

### Cost Estimate (realistic, ~5,000 repos)

Early $1.2k/mo sketches were **too low by ~10×**. Properties, HA replicas, Qdrant overhead, and networking dominate.

| Component | Monthly (realistic) |
|-----------|---------------------|
| Graph primary 128–256GB (+ read replica / HA) | ~$2,000–6,000 |
| Qdrant cluster (~50M+ vectors + overhead) | ~$1,000–3,000 |
| CI / workers (steady; backfill weeks higher) | ~$200–500 |
| PostgreSQL (registry, ACL, jobs) | ~$200 |
| Query + ingest + queue services | ~$300–500 |
| Object storage + data transfer | ~$200–500 |
| Networking / ALB / monitoring | ~$200–400 |
| **Total** | **~$5,000–12,000/mo** |
| **~10k repos** | **~$10,000–20,000/mo** |

Still strong ROI vs engineer time; do **not** socialize the $1.2k figure.

**RAM sizing note:** bare Memgraph structure (~204B/node + ~154B/edge) understates cost. With 10+ Symbol properties (`name`, `kind`, `file`, `docstring`, `repo_slug`, `bu`, `team`, …) expect **~400–600B/node** in practice → **128–256GB** for the 5k-repo scenario, not 64GB.

---

## Phase 0 decisions (ACL & topology)

### ACL (required before Phase 1)

| Option | Pros | Cons | Likely at Intuit |
|--------|------|------|------------------|
| **Org-wide read** | Simplest queries; best blast-radius UX | Symbol/API names may be sensitive; SOX/PCI posture | Unlikely as default |
| **BU-scoped read** | Matches org boundaries; simpler than per-repo | Cross-BU product work still needs exceptions | Possible default |
| **GitHub-mirror (per-repo)** | Matches existing access; strongest compliance | Every hop ACL-filtered; query service is a trust boundary; org blast-radius constrained | **Most likely** |

**Implication if GitHub-mirror:** query service must push `allowed_repo_slug` into graph/vector filters **before** expansion; cross-boundary edges stop at redacted stubs. Cross-BU “who calls this?” may only work for principals with access to both sides — product copy must say so.

### Topology: one graph vs BU federation

| | **One logical graph** (default in this doc) | **BU graphs + contract registry** |
|--|---------------------------------------------|-----------------------------------|
| Cross-BU traversal | Single Cypher | Two-hop via registry summaries |
| RAM ceiling | Single Memgraph primary must hold all | Ceiling per BU |
| ACL | Filter in query | Isolation by default |
| Ops | One cluster | Many clusters + federation layer |
| Fit if | Cross-BU deep hops are common | ≥95% queries stay in-BU |

**Decision rule:** start with **one logical graph** for product simplicity **only if** Phase 0 accepts the Memgraph ceiling plan below. If compliance demands hard BU isolation, or early metrics show &lt;5% cross-BU deep queries, prefer **federation** and keep cross-BU as contract-level (not full symbol graph).

### Memgraph ceiling contingency (P0)

When (not if) single-node RAM or write capacity is exceeded:

1. Vertical scale primary (128→256→512GB) + read replicas — first lever.
2. **Virtual-repo** split for monorepos (bounded replace units).
3. **Physical BU shards**: each BU’s symbols in its own Memgraph; Postgres contract registry holds cross-BU `Repo DEPENDS_ON` / exported API stubs; query service fans out and stitches. Deep cross-BU symbol hops become multi-query, not one Cypher.
4. Neptune / distributed graph only if federation ops cost exceeds managed graph cost.

Document the trigger: e.g. primary RAM &gt; 80% for 7 days, or ingest p95 lag &gt; SLO — then execute step 3.

---

## Scaling Challenges & Mitigations

Sizing alone is not a plan. These are the issues that can **block rollout** if deferred.

### 1. Memgraph RAM + single-writer — quiet showstopper

| Risk | Reality |
|------|---------|
| Doc-era 64GB estimate | Barely fits structure-only math; **properties blow this up** → plan **128–256GB** for ~5k repos |
| Read replicas | Help QPS, **not** primary RAM or write capacity |
| Outgrowing one box | No native write shard — need contingency above |

### 2. Monorepo reindex is an availability event

Uniform “10k symbols/repo” is wrong for QuickBooks-class trees (500k–2M symbols). Full delete+insert ≈ millions of mutations; with 50+ merges/day, full replace becomes continuous churn and queued jobs go stale.

**v1 requirement:** patch ingest for large repos; dual-buffer / epoch pointer so readers never see empty mid-swap. Virtual-repo partitions optional but recommended for the largest trees.

### 3. Platform-package link thundering herd

“Re-link only dependents” is still O(dependents). Auth/logging SDKs can have thousands of dependents.

**v1 requirement:** classify high-fan-out providers; **lazy / eventual** linking; coalesce + rate-limit; publish link freshness SLO separate from code-index freshness.

### 4. Hub symbols / traversal explosion

Hop limits, result caps, default `team`/`bu` scope, optional hub truncation — as before.

### 5. Index lag, backpressure, kill switches

At high merge rates the pipeline must **degrade gracefully**:

| Condition | Action |
|-----------|--------|
| Graph primary RAM/CPU over SLO | Circuit-break: pause backfill + low-priority ingest; keep interactive reindex |
| Queue age &gt; threshold | Alert; drop or coalesce duplicate repo jobs (newest SHA wins) |
| Poison artifact | DLQ; do not block the repo forever without human ack |
| Query memory/time | Hard timeout in query service — protect Memgraph from bad Cypher |

Unbounded “queue forever” is not a strategy.

### 6. ACL is load-bearing (not a late open question)

See Phase 0. Wrong choice late means rewiring query service, caching, and product promises.

### 7. Vector / search (Qdrant)

Few collections + payload filters; no 5k collections; embed by content-hash; require scope filters on org search.

### 8. Cost honesty

Use the **$5–12k/mo** band for planning. Understating cost kills trust more than the spend itself.

### 9. Target SLOs

| Metric | Target (v1 stretch) |
|--------|---------------------|
| Push → searchable (non-monorepo) | p50 &lt; 5 min, p95 &lt; 20 min |
| Push → searchable (monorepo patch) | p95 &lt; 30 min |
| Platform link freshness | p95 &lt; 1 h (eventual OK) |
| `trace_callers` depth≤3, scope=team | p95 &lt; 500 ms |
| `search` scope=bu, limit=20 | p95 &lt; 1 s |
| Ingest queue age | Alert &gt; 30 min |
| Graph primary RAM | Alert &gt; 80% |

### Scaling decision ladder

```
Phase 0: ACL + topology + cost envelope locked
Day 1:   one logical graph (or BU federation if Phase 0 says so)
         + patch ingest for monorepos
         + scoped queries + hop caps + backpressure
         ↓ if RAM / write lag
Bigger primary + read replicas + ingest concurrency caps
         ↓ if monorepos still dominate
Virtual-repo partitions
         ↓ if ceiling or compliance
Physical BU shards + contract-registry federation
```

---

## Open Questions

1. **ACL model** — Phase 0; default assumption **GitHub-mirror** unless security signs org/BU-wide read. *(Was incorrectly listed as deferrable.)*
2. **Topology** — confirm single graph vs BU federation with a sample of real query mixes / compliance.
3. **Monorepo virtual-repo cut lines** — package path? CODEOWNERS? manual allowlist?
4. **Private forks** — independent subgraphs or share parent?
5. **Retention** — latest only in hot graph (recommended); S3 for history.
6. **Memgraph vs Neptune** — only after ceiling contingency is costed; don’t pick Neptune just to avoid ops if federation is cheaper.
7. **Platform-provider threshold** — dependents count / name allowlist for lazy link?
8. **Embedding model** — keep model2vec-rs vs code-specialized model.
9. **PR preview indexes** — after default-branch path is stable?
10. **Default branch override UX** — admin API vs manifest on default branch?
11. **Hub-node policy** — auto-detect truncate vs caps only?
12. **Exact RAM proof** — run a 1–2 BU pilot and measure bytes/node with full properties before buying 512GB boxes.

