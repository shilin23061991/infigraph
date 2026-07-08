# Design: Language-Agnostic Cross-Repo Linking

Status: draft
Author: (session 2026-07-08)
Scope: how cross-repo HTTP/service edges are produced and matched — make it correct
and uniform across all supported languages instead of name-guessing.

## TL;DR

> **CORRECTION (2026-07-08, after reading the live pipeline):** the producer side is
> NOT name-guessing. The contract pipeline is `sync_group_contracts → extract_contracts`
> (multi/mod.rs:200,418), which **already** parses the real decorator path
> (`parse_route_from_docstring`) and joins the router prefix (`extract_router_prefix`).
> Verified against the live contract dump: `ascend-service POST /v1/qb/reports/report-run`
> — full path + prefix + correct method. **1a and 1b are already implemented and
> working for the whole decorator/annotation family.** `routes/*.rs::detect_routes` (the
> name-guesser this doc originally indicted) is a *separate, unused-for-contracts*
> feature; editing it would change zero eval results. Remaining producer gap is only
> **1c** (call-registration frameworks: Express `app.get('/x',fn)`, Go `HandleFunc`,
> bare NestJS `@Get(` — not matched by mod.rs:229's decorator filter). The real
> remaining eval gap (Q5) is **consumer-side edge attribution**, see below.

Original framing (superseded, kept for context): the concern "this only fixes Python"
is half-right — the linking is not Python-specific, it is *path-quality*-specific. What
was assumed to be pervasive name-guessing is, on the live path, already real-path
extraction for decorator frameworks. The holistic principle still holds — **symmetry via
real paths** — but for decorator frameworks it is already achieved.

## Current architecture (as-is)

Two independent halves meet in `multi/cross_service.rs::detect_cross_service_deps`.

### Producer side — "what routes does service B expose?"
`routes/mod.rs::detect_routes` → per-language dispatch
(`routes/{python,go,java,js_ts,rust,ruby,php,csharp,elixir,generic}.rs`).

Each detector receives only `(id, name, name_lower, file, doc_lower)` — pulled from
the graph's `Symbol.name` and `Symbol.docstring`. It then **guesses**:

- `routes/python.rs`: `get_report_filters` → `/report/filters`; Django CBV method
  `get` → path from class name stripped of `view`/`viewset`.
- Path is documented as "best-effort from symbol/docstring heuristics" (`routes/mod.rs:37`).

**Evidence of the defect** (ascend-service, `app/routers/qb/reports/report_router.py`):

```python
router = APIRouter(prefix="/v1/qb/reports")          # line 42

@router.post("/report-run", operation_id="run_report")   # line 68
async def run_report(...):                                 # line 102
```

- Real route: `POST /v1/qb/reports/report-run`.
- Producer sees function name `run_report`, no `post_` prefix → misses it or guesses
  `/report/run` — neither matches the real path.
- `search_symbols("report-run")` in the indexed graph returns **no `Route`-kind
  symbol** → the decorator was never captured for this file.

### Consumer side — "who calls service B?"
`multi/cross_service.rs`:
- `scan_source_for_urls` / `extract_api_paths`: re-read raw source, pull **real** URL
  string literals (`"/v1/..."`, f-strings, `http://.../v1/...`), infer method from
  call syntax (`.post(`, `method="PUT"`). This is genuinely multi-language (already
  handles py/go/ts/js patterns).
- `normalize_route_path`: `:id`/`{id}`/`<id>` → `*`; strips host; wildcard prefixes.
- Matched against `route_lookup` built from producer contracts.

### The asymmetry (root cause)
Consumer reads **real** paths; producer emits **guessed** paths. Match = guess ∩ real.
That intersection is small and full of accidental collisions.

### Infrastructure that already exists but is disconnected
`extract/entities.rs` already defines grammar capture names
`@route.def / @route.method / @route.path / @route.handler` (lines 16, 150-161) and
**already builds a `SymbolKind::Route` symbol** from them with the real path
(lines 236-272).

**Key empirical finding — the real path is ALREADY in the graph, just unparsed.**
The Python grammar query (`languages/python/entities.scm`) captures the
`@router.post(...)` decorator as `@func.decorator` (line 14), and
`extract/entities.rs:173-179` **prepends the full decorator text to the docstring**.
`get_doc_context` for `run_report` returns a docstring that literally begins:

```
@router.post(
    "/report-run",
    operation_id="run_report",
    ...
```

So `POST` and `/report-run` are present verbatim on the symbol. Nobody parses them.

Gaps, in order of cost:
1. **Nobody parses the decorator's method+path arg** out of the docstring text. The
   producer's `detect_from_docstring` looks for framework *keywords*, then falls to
   name-guessing — it never extracts `.post("/report-run")`. **(cheapest fix — data is
   already there.)**
2. The router **`prefix` is not joined** — `/report-run` needs its `/v1/qb/reports`
   prefix, which lives on a separate `router = APIRouter(prefix="/v1/qb/reports")`
   symbol (also captured, as a `var`).
3. `@route.*` grammar captures only fire for **Django `path()`/`re_path()`**
   (entities.scm:73-80). FastAPI/Flask/NestJS/etc. decorator routes are not emitted as
   `SymbolKind::Route` — they only survive as decorator-in-docstring text.
4. `detect_routes` **ignores `SymbolKind::Route` symbols entirely** — only scans
   `Function`/`Method` names.

## Design (to-be)

Principle: **one structured route fact per real endpoint, extracted per-language at
index time, consumed uniformly.** Stop guessing.

### Phase 1 — Producer parses real paths from source

**Two framework families need two mechanisms.** This is the crux of the "does it work
for all languages" question: it works for all *decorator/annotation* frameworks (1a+1b),
and *call-registration* frameworks (Express, Go stdlib) need path 1c — but the raw
material for both already exists.

**1a + 1b are ONE unit — do not split them.** For a prefixed router, 1a alone produces
`/report-run` while the consumer scans the *full* real URL `/v1/qb/reports/report-run`
(verified: Q11 consumers call `ascend-agent POST /v1/labrador/upload`, full path with
prefix). `/report-run` matches nothing without the prefix. Prefix-join is **mandatory**,
not optional hardening.

1a. **Parse method+path from the decorator/annotation text already on the docstring.**
   `extract/entities.rs:173-179` prepends decorator/attribute text to the docstring, so
   `@router.post("/report-run"...` is already on the `run_report` symbol
   (**verified via `get_doc_context`**). In the per-language producer
   (`routes/python.rs` etc.), before name-guessing, parse method+path from that prefix.
   Generalizes across languages for the **decorator/annotation family**: FastAPI/Flask
   `@router.post("/x")`, Spring `@GetMapping("/x")`, NestJS `@Get("/x")`, actix
   `#[get("/x")]` — all arrive as decorator/attribute text via the same
   `find_preceding_attributes` / decorator-capture path. **Does NOT cover
   call-registration frameworks** (see 1c).

1b. **Join the router prefix (mandatory for 1a).** Capture
   `APIRouter(prefix="/v1/qb/reports")` (a `var` symbol in the same file) /
   `express.Router()` mount / Go subrouter prefix and prepend to the decorator path so
   the stored path is the full `/v1/qb/reports/report-run`. Prefix + decorator live in
   separate symbols → per-file post-pass keyed by the `router` variable. **Check the
   cross-file mount case** (`app.include_router(router, prefix=...)`).

1c. **Call-registration frameworks — reuse the consumer scanner.** Express
   `app.get('/v1/x', fn)`, Go `mux.HandleFunc("/v1/x", fn)` have **no decorator**; the
   path is a call argument and the handler is often anonymous, so 1a misses them
   entirely. But the path string is a literal in source — and `scan_source_for_urls`
   (consumer side) **already extracts real path literals from source**. Point the same
   scanner at the producer side (registration calls) to emit routes for this family.
   Optionally formalize via `@route.*` grammar captures (currently only Django
   `path()`/`re_path()` fires, entities.scm:73-80) — this is where long-term
   per-framework extensibility lives: **add a framework = add its route pattern**, not
   edit `cross_service.rs`.

2. **`detect_routes` prefers real routes over name-guessing.** When a symbol has a
   parsed route (1a+1b) or a scanned/`SymbolKind::Route` fact (1c), use it; fall back to
   the name heuristic only for symbols with neither. Keep the guesser as fallback, not
   primary.

### Phase 2 — Consumer symmetry (optional, later)

The consumer already reads real URLs. Once producers emit real paths, matching is
real-vs-real. Optionally unify both onto index-time structured facts (emit a
`ServiceCall` fact analogous to `Route`) so neither side re-scans text at link time.
Flagged as a **second phase** — do not bundle with Phase 1.

### Why this generalizes

| | Before | After |
|---|---|---|
| Add a framework | Add regex/patterns to `cross_service.rs` + a `routes/*.rs` name-guesser | Decorator family: parse existing decorator text. Call-reg family: reuse source scanner / add route pattern |
| Route path source | Guessed from function name | Real path string from source (decorator arg or call-arg literal) |
| Match basis | guess ∩ real (collision-prone) | real ∩ real |
| False matches | 151 name-collisions at v2.0.0 | eliminated at source |

## The actual remaining gap — consumer-side edge attribution (Q5)

Producer contracts are correct (verified above), so Q5's partial score is **not** a
path problem. The `CALLS_SERVICE` edges attach to the wrong node.

**Verified** (`ascend-agent` graph):

```
MATCH (c)-[:CALLS_SERVICE]->(t) WHERE t.name CONTAINS 'estimates' OR 'schedules'
→ app/adapters/ascend_service_client.py::AscendServiceClient | Class | POST /v1/entities/schedules
→ app/adapters/ascend_service_client.py::AscendServiceClient | Class | GET  /v1/entities/estimates
```

The caller node is the **class `AscendServiceClient`**, not the **method** that makes
each call (`get_estimates`, `post_schedules`). When search surfaces an individual
method, it carries none of the class's cross-repo edges → Q5 shows the route but not the
per-method consumer, scoring partial.

Root cause: `caller_symbol` resolution in `link_cross_service_calls`
(`multi/cross_service.rs`) — the line-hint → enclosing-symbol query resolves to the
class span rather than the innermost method. Fix = resolve to the smallest enclosing
`Function`/`Method`, not the class. **This is a separate task from Phase 1** (different
file, different mechanism); get explicit go-ahead before starting — the producer-path
premise this doc opened with does not apply here.

## Q7 — out of static scope (verified, attempted, reverted)

Q7 ("QBO report filter schema, who reads it") stays unsolved at 11/12. The consumer is
`ascendskills/tools/__init__.py::ToolsServer::register_api_tools`, which at runtime
fetches `{ascend_svc_url}/openapi.json` and registers one MCP tool per route
(`ascend_api.register`). It is a **query-blind wildcard consumer** of the *entire*
ascend-service route surface — it has zero report-filters-specific text. Verified three
ways:
1. Per-repo search on ascend-skills for the Q7 query returns only `*.schema.json` files;
   the consumer never surfaces (no route-specific text to match).
2. `base_url` is runtime-configured — no static literal points at a specific route.
3. The per-route tool names/descriptions are generated at runtime from the fetched spec.

**Attempted fix (reverted):** a service-level wildcard edge
`register_api_tools --CALLS_SERVICE(path='*')--> ascend-service`, surfaced via
diversity-gated injection at low score. It failed the gate: the diversity swap promotes
the starved repo's *best real hit* (`garden.schema.json`, 0.21), which outscores any
principled low injection score. Raising the injection score to win the slot is the
rejected floor-boost hack — and because the consumer is query-blind, any score that
wins Q7 also sprays it across the ~9 questions with an ascend-service route in top-N.
There is no query-agnostic score that fixes Q7 without regressing others. Reverted to
the clean 11/12 (`3d9d7c0`).

Note: how codebase-memory reportedly gets Q7 was never confirmed; the evidence above
suggests plain retrieval cannot surface this consumer, so its mechanism remains an
open question, not a target to match.

## Non-goals / constraints

- **Do not touch the single-repo search path.** Diversity/scoring work stays in
  `multi/combined.rs`. This design is about *edge production*, orthogonal to search
  ranking.
- **Do not modify `.g4` / externally-owned grammar files.** Route capture patterns go
  in the tree-sitter *query* files infigraph owns, not vendored grammars.
- Runtime-only links (Q7: MCP tool reads a schema at runtime) remain out of scope —
  no static edge exists; document as a known limitation.

## Open questions before implementing

1. ~~Where do the per-language tree-sitter query files live, and do any already emit
   `@route.*`?~~ **Answered:** `crates/infigraph-languages/languages/{lang}/entities.scm`.
   Only Python's Django `path()`/`re_path()` emits `@route.*` (entities.scm:73-80).
   Decorator routes survive as decorator-text-on-docstring, not structured routes.
2. Prefix-join: is a single-file post-pass enough, or do routers get mounted across
   files (`app.include_router(router, prefix=...)`)? ascend-service uses in-file
   `APIRouter(prefix=...)` — check the cross-file mount case.
3. Should `Route` symbols be first-class in search results, or purely internal fuel for
   `detect_routes`? (Affects whether they get embeddings.)

## Prior art — and why none of it is a drop-in

No existing framework produces the thing this design actually needs: the **cross-repo
`call → route` edge**. Each tool below hands you *one side* of the join; matching a
consumer's real URL to a producer's real route and writing the graph edge stays
infigraph's job regardless.

- **OpenAPI/Swagger specs.** FastAPI/Spring/NestJS produce `openapi.json` with
  *fully-resolved* paths (prefix joined) + methods + `operationId` — exactly the
  producer data we want. **But it is generated at runtime, not committed.** Verified:
  ascend-skills obtains it via `httpx.get(f"{base_url}/openapi.json")`
  (`ascendskills/tools/ascend_api.py::register`) — a live fetch against the running
  service, no spec file in-tree. So for a *static* indexer OpenAPI shares the same flaw
  as route-dumpers below: needs the app running. **Use only if a spec file is committed
  to the repo; otherwise not available at index time.**
- **Framework route dumpers** — `rails routes`, Spring actuator `/mappings`,
  `express-list-endpoints`, `fastapi` app introspection. Runtime-accurate, require the
  app running. Same disqualifier for static indexing.
- **Semantic/SCIP/LSIF indexers** (Sourcegraph) resolve symbols cross-repo but do *not*
  model HTTP-call→route edges. Hands you symbol resolution, not the service link.
- **codebase-memory** (scored 11/12) does not parse routes structurally; it runs
  per-repo searches and joins by hand. Its win was retrieval breadth, not route
  extraction — orthogonal to this design.

Implication: **the primary fix is Phase 1 — parse the real path from the decorator text
already in the graph, statically, no running app.** OpenAPI/dumpers are an opportunistic
fast path *only when a resolved spec is committed to the repo*, not the default.

## Validation plan

- Reindex ascend-service; assert a `Route` symbol `POST /v1/qb/reports/report-run` exists.
- Re-run 12-Q cross-repo eval; expect Q5 (estimates/schedules rename) to move from
  partial → correct once real method-level route paths replace class-level guesses.
- Regression: single-repo search results unchanged (byte-diff top-10 on a fixed query
  set before/after).
