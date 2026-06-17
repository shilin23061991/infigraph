# Pipeline Plugins

Infigraph supports runtime-loaded pipeline plugins for extracting data pipeline metadata from documents (e.g. Confluence design docs). Each plugin is a subprocess that communicates via JSON over stdin/stdout — no recompilation needed to add new pipeline formats.

## Architecture

```
Document content
    │
    ▼
PipelinePluginRegistry.extract_auto(content, title, doc_id)
    │
    ├── detect_patterns match? ──► Plugin subprocess (JSON IPC)
    │                                    │
    │                                    ▼
    │                              PipelineData {
    │                                core: { name, inputs, outputs },
    │                                properties: { key: value }
    │                              }
    │                                    │
    ▼                                    ▼
DocStore                           DocStore
    ├── upsert_pipeline_core()     ├── upsert_plugin_properties()
    │   → PipelineCore node        │   → Pipeline_<plugin_id> node
    ├── link_pipeline_to_doc()     └── link_pipeline_dependencies()
    │   → DEFINED_IN edge              → DEPENDS_ON edges
    └── impact_analysis()
        → transitive impact
```

## How It Works

1. Plugin is a directory with `plugin.toml` + an extractor binary/script
2. Infigraph discovers plugins at startup from two locations:
   - `~/.infigraph/pipelines/*/plugin.toml` — user-level (all projects)
   - `<project>/pipelines/*/plugin.toml` — project-level (per repo)
3. When a document is indexed, infigraph tries each plugin's `detect_patterns` against the content
4. Matching plugin's extractor subprocess receives the document, returns structured pipeline metadata
5. Metadata is stored in the graph DB: `PipelineCore` shared table + plugin-specific detail table

## Directory Structure

```
~/.infigraph/pipelines/          <project>/pipelines/
       │                                │
       ├── intuit/                      ├── dbt/
       │     plugin.toml                │     plugin.toml
       │                                │     extract.sh
       ├── airflow/                     └── custom/
       │     plugin.toml                      plugin.toml
       │     extract.js                       extract
       └── ...
```

Plugins in `<project>/pipelines/` override user-level plugins with the same `plugin_id`.

## plugin.toml Format

```toml
[plugin]
name = "My Pipeline Format"
plugin_id = "myformat"                        # lowercase a-z, 0-9, underscore; max 32 chars
command = ["./extract"]                       # any executable that speaks the JSON protocol
detect_patterns = ["## Source Systems", "## Scheduler"]  # regex patterns for auto-detection
searchable_fields = ["compliance", "business_logic_summary"]  # fields indexed for search

# Per-plugin table columns (creates Pipeline_myformat node table in the graph DB)
[[plugin.schema]]
name = "scheduler_type"
col_type = "STRING"    # STRING | INT64 | BOOL | DOUBLE | STRING[]

[[plugin.schema]]
name = "compliance"
col_type = "STRING"

[[plugin.schema]]
name = "owner"
col_type = "STRING"

[plugin.dependency_fields]
inputs = "core.inputs"     # field path for upstream dependencies
outputs = "core.outputs"   # field path for downstream outputs
```

### Validation Rules

- `plugin_id`: must match `^[a-z][a-z0-9_]{0,31}$`
- Column names: must match `^[a-z][a-z0-9_]{0,63}$`
- `col_type`: must be one of `STRING`, `INT64`, `BOOL`, `DOUBLE`, `STRING[]`
- `command`: must be non-empty

## JSON IPC Protocol

Communication between infigraph and plugin extractors uses newline-delimited JSON over stdin/stdout.

### Startup Handshake

When started, the plugin writes to stdout:

```json
{"ready": true, "plugin_id": "myformat", "version": "1.0"}
```

### Extract Command

Infigraph sends to plugin stdin:

```json
{"command": "extract", "content": "full document text...", "title": "My Pipeline", "doc_id": "doc::123"}
```

### Response (plugin → stdout)

**Success:**
```json
{
  "status": "ok",
  "data": {
    "core": {
      "name": "My Pipeline",
      "inputs": ["src.table_a", "ref.dim_b"],
      "outputs": ["dm.fact_c"]
    },
    "properties": {
      "scheduler_type": "Airflow",
      "compliance": "SOX",
      "owner": "data-team"
    }
  }
}
```

**Skip** (document doesn't match this plugin):
```json
{"status": "skip"}
```

**Error:**
```json
{"status": "error", "message": "failed to parse document"}
```

## Schema: Shared Core + Per-Plugin Tables

### PipelineCore (shared across all plugins)

```
PipelineCore node table:
  id         STRING (PRIMARY KEY)  — e.g. "pipeline::w2_metrics"
  name       STRING                — human-readable name
  doc_id     STRING                — link to source document
  plugin_id  STRING                — which plugin created this
  inputs     STRING[]              — upstream table/dataset dependencies
  outputs    STRING[]              — downstream tables this pipeline produces
```

### Per-Plugin Detail Tables

Each plugin gets a dynamically created table `Pipeline_<plugin_id>` with columns from the schema in `plugin.toml`:

```
Pipeline_intuit node table:
  id                     STRING (PRIMARY KEY)  — same as PipelineCore.id
  scheduler_type         STRING
  scheduler_config       STRING
  compliance             STRING
  github_repo            STRING
  daci                   STRING
  ...
```

Joined via `PipelineCore.id = Pipeline_<plugin_id>.id`.

### Edge Tables

```
DEFINED_IN:  PipelineCore → Document    (which doc defined this pipeline)
DEPENDS_ON:  PipelineCore → PipelineCore (dep_type: "input-output")
```

### Cross-Plugin Dependencies

Dependency linking works across plugins. If an Intuit pipeline outputs `staging.processed` and a dbt pipeline lists `staging.processed` as an input, infigraph creates a `DEPENDS_ON` edge between them automatically.

```
Intuit ETL (outputs: [staging.processed])
    │
    ▼ DEPENDS_ON
dbt Transform (inputs: [staging.processed])
    │
    ▼ DEPENDS_ON
Analytics Dashboard (inputs: [analytics.metrics])
```

## MCP Tools

| Tool | Description | Parameters |
|------|-------------|------------|
| `pipeline_plugins` | List loaded pipeline plugins | `path` |
| `pipeline_deps` | Show pipeline dependency graph | `path` |
| `pipeline_impact` | Transitive impact analysis for a table/dataset | `path`, `table_name`, `max_depth?` |
| `pipeline_compliance` | Query pipelines by compliance scope | `path`, `scope`, `plugin_id?` |
| `pipeline_query` | Generic query against plugin-specific fields | `path`, `plugin_id`, `field`, `value` |

### Example: Impact Analysis

Query which pipelines are affected when `tax_src.raw_w2_data` changes:

```
pipeline_impact(path="/myproject", table_name="tax_src.raw_w2_data", max_depth=3)
```

Returns:
```
3 pipelines impacted by 'tax_src.raw_w2_data':

  [depth=1] W2 Metrics (direct) — tax_src.raw_w2_data → W2 Metrics
  [depth=2] Marketing Attributes (transitive) — via W2 Metrics
  [depth=3] Deceased Taxpayer Filter (transitive) — via Marketing Attributes
```

## Built-in Extractor

Infigraph ships `infigraph-pipeline-extractor` — a reference implementation that extracts pipeline metadata from Confluence-style design documents with sections like:

- **Source Systems** → `inputs`
- **Destination Tables** → `outputs`
- **Scheduler** → `scheduler_type`, `scheduler_config`
- **Compliance** → `compliance` (e.g. IRS 7216, SOX)
- **DACI** → `daci` (decision roles)
- **Business Logic** → `business_logic_summary`

It ships as a compiled Rust binary with zero runtime dependencies.

Configuration: `pipelines/intuit/plugin.toml`

## Writing a Custom Plugin

### Step 1: Create plugin directory

```bash
mkdir -p ~/.infigraph/pipelines/my-plugin
```

### Step 2: Write plugin.toml

```toml
[plugin]
name = "My Custom Pipeline Format"
plugin_id = "myplugin"
command = ["./extract"]
detect_patterns = ["## Data Sources", "Pipeline:"]

[[plugin.schema]]
name = "environment"
col_type = "STRING"

[[plugin.schema]]
name = "sla_hours"
col_type = "INT64"

[plugin.dependency_fields]
inputs = "core.inputs"
outputs = "core.outputs"
```

### Step 3: Implement extractor

Any language works. Here's a minimal example in Bash:

```bash
#!/bin/bash
# extract — reads JSON commands from stdin, writes JSON to stdout

# Startup handshake
echo '{"ready": true, "plugin_id": "myplugin", "version": "1.0"}'

# Read commands in a loop
while IFS= read -r line; do
  command=$(echo "$line" | jq -r '.command')
  if [ "$command" = "extract" ]; then
    title=$(echo "$line" | jq -r '.title')
    # Parse your document format here...
    echo "{\"status\": \"ok\", \"data\": {\"core\": {\"name\": \"$title\", \"inputs\": [\"raw.data\"], \"outputs\": [\"clean.data\"]}, \"properties\": {\"environment\": \"prod\", \"sla_hours\": 4}}}"
  fi
done
```

### Step 4: Test

```bash
chmod +x extract
echo '{"command":"extract","content":"## Data Sources\nraw.events table","title":"My Pipeline","doc_id":"test::1"}' | ./extract
```

### Step 5: Verify in infigraph

```bash
infigraph index        # re-index to pick up new plugin
# Use MCP tool:
# pipeline_plugins(path=".") → should show your plugin
# pipeline_query(path=".", plugin_id="myplugin", field="environment", value="prod")
```

## Example: dbt Plugin

A dbt plugin could parse `schema.yml` and `dbt_project.yml`:

```toml
[plugin]
name = "dbt Models"
plugin_id = "dbt"
command = ["python3", "extract_dbt.py"]
detect_patterns = ["models:", "source\\("]

[[plugin.schema]]
name = "materialization"
col_type = "STRING"

[[plugin.schema]]
name = "tags"
col_type = "STRING[]"

[plugin.dependency_fields]
inputs = "core.inputs"
outputs = "core.outputs"
```

The extractor would parse dbt manifest/catalog JSON and return pipeline metadata in the standard format.
