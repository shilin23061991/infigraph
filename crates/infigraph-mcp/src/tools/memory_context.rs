use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::Value;

use infigraph_core::embed;
use infigraph_core::graph::SessionStore;

use super::helpers::open_prism;

#[derive(Clone)]
struct ScoredItem {
    id: String,
    source_type: SourceType,
    score: f32,
    rendered_text: String,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SourceType {
    Code = 0,
    Skeleton = 1,
    Session = 2,
}

pub fn tool_memory_context(args: &Value) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|p| p.as_str())
        .context("missing 'path'")?;
    let query = args
        .get("query")
        .and_then(|q| q.as_str())
        .context("missing 'query'")?;
    let anchor_file = args.get("file").and_then(|f| f.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let sources_str = args
        .get("sources")
        .and_then(|s| s.as_str())
        .unwrap_or("code,sessions,skeleton");
    let want_code = sources_str.contains("code");
    let want_sessions = sources_str.contains("sessions");
    let want_skeleton = sources_str.contains("skeleton");

    let mut items: Vec<ScoredItem> = Vec::new();
    let mut always_include: Vec<ScoredItem> = Vec::new();

    if want_code {
        items.extend(gather_code(args, query, anchor_file, limit)?);
    }

    if want_sessions {
        let (session_items, constraint_items) = gather_sessions(path, query)?;
        items.extend(session_items);
        always_include.extend(constraint_items);
    }

    let skeleton_item = match anchor_file {
        Some(f) if want_skeleton => gather_skeleton(args, f)?,
        _ => None,
    };

    if let Some(f) = anchor_file {
        if want_code {
            apply_anchor_boost(&mut items, args, f)?;
        }
    }

    let ranked = rank_and_assemble(items, skeleton_item, always_include);
    render_output(query, &ranked)
}

fn gather_code(
    args: &Value,
    query: &str,
    anchor_file: Option<&str>,
    limit: usize,
) -> Result<Vec<ScoredItem>> {
    let prism = open_prism(args)?;
    let path = args.get("path").and_then(|p| p.as_str()).unwrap_or(".");
    let store = prism
        .store()
        .context("not indexed — run index_project first")?;
    let conn = store.connection()?;
    let gq = infigraph_core::graph::GraphQuery::new(&conn);

    let rows = gq.raw_query(
        "MATCH (s:Symbol) RETURN s.id, s.name, s.kind, s.file, s.docstring, s.start_line, s.end_line",
    )?;

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let docs: Vec<(String, String)> = rows
        .iter()
        .map(|row| {
            let id = row[0].clone();
            let text = if row.get(4).is_some_and(|s| !s.is_empty()) {
                format!("{} {}: {}", row[2], row[1], row[4])
            } else {
                format!("{} {}", row[2], row[1])
            };
            (id, text)
        })
        .collect();

    let bm25_index = infigraph_core::search::BM25Index::build(docs.clone());
    let embedder = embed::best_embedder();
    let emb_path = PathBuf::from(path)
        .join(".infigraph")
        .join("embeddings.bin");
    let symbol_embeddings: Vec<(String, Vec<f32>)> = if emb_path.exists() {
        let all: HashMap<String, Vec<f32>> = embed::load_embeddings_cached(&emb_path)?
            .into_iter()
            .collect();
        docs.iter()
            .filter_map(|(id, text)| {
                all.get(id)
                    .cloned()
                    .or_else(|| embedder.embed(text).ok())
                    .map(|emb| (id.clone(), emb))
            })
            .collect()
    } else {
        docs.iter()
            .map(|(id, text)| (id.clone(), embedder.embed(text).unwrap_or_default()))
            .collect()
    };

    let oversample = limit * 2;
    let tg_dir = PathBuf::from(path).join(".infigraph");
    let hnsw_path = tg_dir.join("hnsw_index.usearch");
    let raw = infigraph_core::search::compute_raw_scores(
        query,
        &bm25_index,
        embedder.as_ref(),
        &symbol_embeddings,
        oversample,
        Some(&hnsw_path),
        Some(&emb_path),
    )?;

    let keyword_results = infigraph_core::search::combine_scores(&raw, 0.3, limit);
    let semantic_results = infigraph_core::search::combine_scores(&raw, 0.85, limit);

    let mut merged: HashMap<String, infigraph_core::search::SearchResult> = HashMap::new();
    for r in keyword_results.into_iter().chain(semantic_results) {
        merged
            .entry(r.symbol_id.clone())
            .and_modify(|existing| {
                if r.score > existing.score {
                    *existing = r.clone();
                }
            })
            .or_insert(r);
    }

    let mut results: Vec<infigraph_core::search::SearchResult> = merged.into_values().collect();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    let row_map: HashMap<&str, &Vec<String>> =
        rows.iter().map(|row| (row[0].as_str(), row)).collect();

    let mut scored_items = Vec::new();
    for r in &results {
        let row = match row_map.get(r.symbol_id.as_str()) {
            Some(row) => row,
            None => continue,
        };

        let start_line: usize = row.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
        let end_line: usize = row.get(6).and_then(|s| s.parse().ok()).unwrap_or(0);
        let file = &row[3];
        let kind = &row[2];
        let name = &row[1];
        let docstring = row.get(4).filter(|s| !s.is_empty());

        let mut text = format!("#### {}::{} (score: {:.3})\n", file, name, r.score);
        text.push_str(&format!(
            "Kind: {} | File: {}:{}-{}\n",
            kind, file, start_line, end_line
        ));
        if let Some(doc) = docstring {
            text.push_str(&format!("Doc: {}\n", doc));
        }

        let file_path = prism.root().join(file);
        if let Ok(source) = std::fs::read_to_string(&file_path) {
            let lines: Vec<&str> = source.lines().collect();
            let start = start_line.saturating_sub(1);
            let end = end_line.min(lines.len());
            if start < end {
                text.push_str("```\n");
                for (i, line) in lines[start..end].iter().enumerate() {
                    text.push_str(&format!("{:4}  {}\n", start + i + 1, line));
                }
                text.push_str("```\n");
            }
        }

        let callers = gq.callers_of(&r.symbol_id).unwrap_or_default();
        let callees = gq.callees_of(&r.symbol_id).unwrap_or_default();
        if !callers.is_empty() {
            text.push_str(&format!(
                "Callers: {}\n",
                callers
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !callees.is_empty() {
            text.push_str(&format!(
                "Callees: {}\n",
                callees
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        text.push('\n');

        scored_items.push(ScoredItem {
            id: r.symbol_id.clone(),
            source_type: SourceType::Code,
            score: r.score,
            rendered_text: text,
        });
    }

    if let Some(anchor) = anchor_file {
        let file_symbols = gq.symbols_in_file(anchor).unwrap_or_default();
        for sym in file_symbols {
            if scored_items.iter().any(|s| s.id == sym.id) {
                continue;
            }
            let text = format!(
                "#### {}::{} (anchor file)\nKind: {} | Lines: {}-{}\n\n",
                anchor, sym.name, sym.kind, sym.start_line, sym.end_line
            );
            scored_items.push(ScoredItem {
                id: sym.id,
                source_type: SourceType::Code,
                score: 0.3,
                rendered_text: text,
            });
        }
    }

    Ok(scored_items)
}

fn gather_sessions(path: &str, query: &str) -> Result<(Vec<ScoredItem>, Vec<ScoredItem>)> {
    let root = PathBuf::from(path);
    let store = SessionStore::open(&root)?;
    let emb_path = root
        .join(".infigraph")
        .join("sessions")
        .join("embeddings.bin");

    let mut session_items = Vec::new();
    let mut constraint_items = Vec::new();

    if !emb_path.exists() {
        return Ok((session_items, constraint_items));
    }

    let emb_store = embed::load_embeddings(&emb_path)?;
    if emb_store.is_empty() {
        return Ok((session_items, constraint_items));
    }

    let embedder = embed::code_embedder();
    let query_vec = embedder.embed(query)?;
    if query_vec.is_empty() {
        return Ok((session_items, constraint_items));
    }

    let mut scored: Vec<(f32, &str)> = emb_store
        .iter()
        .map(|(id, vec)| (embed::cosine_similarity(&query_vec, vec), id.as_str()))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(5);

    for (score, session_id) in &scored {
        if let Some(session) = store.load(session_id)? {
            let weighted_score = score * 0.7;

            let created = format_epoch(session.created_at);
            let updated = format_epoch(session.updated_at);
            let name_label = if session.name.is_empty() {
                String::new()
            } else {
                format!(" — \"{}\"", session.name)
            };

            let mut text = format!(
                "#### {}{} (relevance: {:.3})\n",
                session.id, name_label, weighted_score
            );
            text.push_str(&format!("Created: {} | Updated: {}\n", created, updated));

            if !session.summary.is_empty() {
                text.push_str(&format!("**Summary:** {}\n", session.summary));
            }
            if !session.pending_tasks.is_empty() {
                text.push_str(&format!("**Pending:** {}\n", session.pending_tasks));
            }
            if !session.decisions.is_empty() {
                text.push_str(&format!("**Decisions:** {}\n", session.decisions));
            }
            text.push('\n');

            session_items.push(ScoredItem {
                id: session_id.to_string(),
                source_type: SourceType::Session,
                score: weighted_score,
                rendered_text: text,
            });

            if !session.constraints.is_empty() || !session.blockers.is_empty() {
                let mut ctext = String::new();
                if !session.constraints.is_empty() {
                    ctext.push_str(&format!(
                        "**Constraint ({}):** {}\n",
                        session.id, session.constraints
                    ));
                }
                if !session.blockers.is_empty() {
                    ctext.push_str(&format!(
                        "**Blocker ({}):** {}\n",
                        session.id, session.blockers
                    ));
                }
                constraint_items.push(ScoredItem {
                    id: format!("{}_constraints", session_id),
                    source_type: SourceType::Session,
                    score: 1.0,
                    rendered_text: ctext,
                });
            }
        }
    }

    Ok((session_items, constraint_items))
}

fn gather_skeleton(args: &Value, file: &str) -> Result<Option<ScoredItem>> {
    let prism = open_prism(args)?;
    let store = prism.store().context("not indexed")?;
    let conn = store.connection()?;
    let gq = infigraph_core::graph::GraphQuery::new(&conn);

    match gq.skeleton(file) {
        Ok(skeleton_text) if !skeleton_text.is_empty() => {
            let text = format!("#### Skeleton: {}\n{}\n", file, skeleton_text);
            Ok(Some(ScoredItem {
                id: format!("skeleton_{}", file),
                source_type: SourceType::Skeleton,
                score: 0.9,
                rendered_text: text,
            }))
        }
        _ => Ok(None),
    }
}

fn apply_anchor_boost(items: &mut [ScoredItem], args: &Value, anchor_file: &str) -> Result<()> {
    let prism = open_prism(args)?;
    let store = match prism.store() {
        Some(s) => s,
        None => return Ok(()),
    };
    let conn = store.connection()?;
    let gq = infigraph_core::graph::GraphQuery::new(&conn);

    let file_symbols = gq.symbols_in_file(anchor_file).unwrap_or_default();
    let anchor_ids: std::collections::HashSet<String> =
        file_symbols.iter().map(|s| s.id.clone()).collect();

    let mut related_ids: std::collections::HashSet<String> = anchor_ids.clone();
    for sym in &file_symbols {
        for caller in gq.callers_of(&sym.id).unwrap_or_default() {
            related_ids.insert(caller);
        }
        for callee in gq.callees_of(&sym.id).unwrap_or_default() {
            related_ids.insert(callee);
        }
    }

    for item in items.iter_mut() {
        if item.source_type == SourceType::Code && related_ids.contains(&item.id) {
            item.score = (item.score + 0.15).min(1.0);
        }
    }

    Ok(())
}

fn rank_and_assemble(
    mut items: Vec<ScoredItem>,
    skeleton: Option<ScoredItem>,
    always_include: Vec<ScoredItem>,
) -> Vec<ScoredItem> {
    // Sort by score descending, then source_type, then id (deterministic)
    items.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.source_type.cmp(&b.source_type))
            .then(a.id.cmp(&b.id))
    });

    let mut result: Vec<ScoredItem> = Vec::new();

    // Always-include first (constraints/blockers)
    result.extend(always_include);

    // Skeleton
    if let Some(skel) = skeleton {
        result.push(skel);
    }

    // All ranked items
    result.extend(items);

    result
}

fn render_output(query: &str, items: &[ScoredItem]) -> Result<String> {
    let mut out = format!("## Memory Context: \"{}\"\n\n", query);

    let code_items: Vec<&ScoredItem> = items
        .iter()
        .filter(|i| i.source_type == SourceType::Code)
        .collect();
    let skeleton_items: Vec<&ScoredItem> = items
        .iter()
        .filter(|i| i.source_type == SourceType::Skeleton)
        .collect();
    let session_items: Vec<&ScoredItem> = items
        .iter()
        .filter(|i| i.source_type == SourceType::Session)
        .collect();

    if !code_items.is_empty() {
        out.push_str(&format!("### Code ({} results)\n\n", code_items.len()));
        for item in &code_items {
            out.push_str(&item.rendered_text);
        }
    }

    if !skeleton_items.is_empty() {
        out.push_str("### Skeleton\n\n");
        for item in &skeleton_items {
            out.push_str(&item.rendered_text);
        }
    }

    if !session_items.is_empty() {
        out.push_str(&format!(
            "### Sessions ({} results)\n\n",
            session_items.len()
        ));
        for item in &session_items {
            out.push_str(&item.rendered_text);
        }
    }

    Ok(out)
}

fn format_epoch(epoch: i64) -> String {
    let days = epoch / 86400;
    let y = (days * 400 / 146097) + 1970;
    let rem = days - ((y - 1970) * 365 + (y - 1969) / 4 - (y - 1901) / 100 + (y - 1601) / 400);
    let (y, rem) = if rem < 0 {
        let y = y - 1;
        let r = days - ((y - 1970) * 365 + (y - 1969) / 4 - (y - 1901) / 100 + (y - 1601) / 400);
        (y, r)
    } else {
        (y, rem)
    };
    let is_leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let months = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0;
    let mut d = rem;
    for (i, &days_in_month) in months.iter().enumerate() {
        if d < days_in_month {
            m = i + 1;
            break;
        }
        d -= days_in_month;
    }
    if m == 0 {
        m = 12;
    }
    format!("{:04}-{:02}-{:02}", y, m, d + 1)
}
