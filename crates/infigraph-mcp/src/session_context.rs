use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

const DEFAULT_STALENESS_WINDOW: usize = 6;
const DEFAULT_TOKEN_BUDGET: usize = 150_000;

static SESSION: Mutex<Option<SessionContext>> = Mutex::new(None);

struct SeenEntry {
    call_seen: usize,
    content_hash: u64,
    tokens_sent: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompressionLevel {
    Off,
    Summary,
    Aggressive,
    Minimal,
}

struct SessionContext {
    seen: HashMap<String, SeenEntry>,
    call_counter: usize,
    staleness_window: usize,
    total_tokens_sent: usize,
    token_budget: usize,
}

impl SessionContext {
    fn new() -> Self {
        let budget = std::env::var("INFIGRAPH_TOKEN_BUDGET")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_TOKEN_BUDGET);
        Self {
            seen: HashMap::new(),
            call_counter: 0,
            staleness_window: DEFAULT_STALENESS_WINDOW,
            total_tokens_sent: 0,
            token_budget: budget,
        }
    }

    fn auto_level(&self) -> CompressionLevel {
        if self.token_budget == 0 {
            return CompressionLevel::Summary;
        }
        let remaining_pct = ((self.token_budget.saturating_sub(self.total_tokens_sent)) as f64
            / self.token_budget as f64
            * 100.0) as usize;
        match remaining_pct {
            71..=100 => CompressionLevel::Off,
            50..=70 => CompressionLevel::Summary,
            20..=49 => CompressionLevel::Aggressive,
            _ => CompressionLevel::Minimal,
        }
    }
}

fn content_key(tool_name: &str, args: &Value) -> String {
    // Key by tool + primary identifier arg
    let id = args
        .get("symbol_id")
        .or_else(|| args.get("symbol"))
        .or_else(|| args.get("query"))
        .or_else(|| args.get("name"))
        .or_else(|| args.get("file"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    format!("{tool_name}:{id}")
}

fn hash_content(s: &str) -> u64 {
    // FNV-1a 64-bit — no dependency needed
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn estimate_tokens(s: &str) -> usize {
    ((s.split_whitespace().count() as f64) * 1.4).ceil() as usize
}

/// Get current compression level based on token budget usage.
pub fn get_compression_level() -> CompressionLevel {
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = guard.get_or_insert_with(SessionContext::new);
    ctx.auto_level()
}

/// Record tokens sent and return updated compression level.
pub fn track_tokens(tokens: usize) -> CompressionLevel {
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = guard.get_or_insert_with(SessionContext::new);
    ctx.total_tokens_sent += tokens;
    ctx.auto_level()
}

/// Apply seen-dedup to already-compressed tool output.
/// Returns the output unchanged if dedup is disabled or content is fresh.
pub fn apply_seen_dedup(compressed: &str, tool_name: &str, args: &Value) -> String {
    if !std::env::var("INFIGRAPH_DEDUP").is_ok_and(|v| v == "1") {
        return compressed.to_string();
    }

    // Don't dedup error responses or tiny outputs
    if compressed.starts_with("Error:") || compressed.starts_with("No ") {
        return compressed.to_string();
    }
    let tokens = estimate_tokens(compressed);
    if tokens < 50 {
        return compressed.to_string();
    }

    let key = content_key(tool_name, args);
    if key.ends_with(':') {
        return compressed.to_string();
    }

    let hash = hash_content(compressed);

    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = guard.get_or_insert_with(SessionContext::new);
    ctx.call_counter += 1;
    let current_call = ctx.call_counter;

    let effective_window = match ctx.auto_level() {
        CompressionLevel::Off => ctx.staleness_window,
        CompressionLevel::Summary => ctx.staleness_window,
        CompressionLevel::Aggressive => ctx.staleness_window.max(8),
        CompressionLevel::Minimal => ctx.staleness_window.max(12),
    };

    if let Some(entry) = ctx.seen.get(&key) {
        let age = current_call - entry.call_seen;
        if entry.content_hash == hash && age <= effective_window {
            // Same content, still fresh — return compact placeholder
            let placeholder = format!(
                "(seen {} call{} ago: {key}, {} tokens — use detail=true to force full output)",
                age,
                if age == 1 { "" } else { "s" },
                entry.tokens_sent
            );
            // Update the seen entry to refresh the call counter
            ctx.seen.insert(
                key,
                SeenEntry {
                    call_seen: current_call,
                    content_hash: hash,
                    tokens_sent: entry.tokens_sent,
                },
            );
            return placeholder;
        }
        // Content changed or stale — fall through to show full + update
    }

    ctx.seen.insert(
        key,
        SeenEntry {
            call_seen: current_call,
            content_hash: hash,
            tokens_sent: tokens,
        },
    );

    compressed.to_string()
}

/// Reset session state (for testing).
#[cfg(test)]
pub fn reset_session() {
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    *guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_session();
        std::env::set_var("INFIGRAPH_DEDUP", "1");
        std::env::remove_var("INFIGRAPH_TOKEN_BUDGET");
        guard
    }

    fn big_output() -> String {
        "word ".repeat(100) // ~140 tokens
    }

    #[test]
    fn test_dedup_same_content_returns_placeholder() {
        let _g = setup();
        let output = big_output();
        let args = json!({"symbol_id": "src/lib.rs::foo"});

        let first = apply_seen_dedup(&output, "get_doc_context", &args);
        assert_eq!(first, output);

        let second = apply_seen_dedup(&output, "get_doc_context", &args);
        assert!(second.starts_with("(seen "));
        assert!(second.contains("get_doc_context:src/lib.rs::foo"));
    }

    #[test]
    fn test_dedup_changed_content_returns_full() {
        let _g = setup();
        let args = json!({"symbol_id": "src/lib.rs::foo"});

        let first = apply_seen_dedup(&big_output(), "get_doc_context", &args);
        assert!(!first.starts_with("(seen"));

        let changed = format!("{} extra", big_output());
        let second = apply_seen_dedup(&changed, "get_doc_context", &args);
        assert!(!second.starts_with("(seen"));
        assert_eq!(second, changed);
    }

    #[test]
    fn test_dedup_stale_returns_full() {
        let _g = setup();
        let output = big_output();
        let args = json!({"symbol_id": "src/lib.rs::foo"});

        apply_seen_dedup(&output, "get_doc_context", &args);

        // Burn through staleness window with other calls
        for i in 0..7 {
            let other_args = json!({"symbol_id": format!("other_{i}")});
            apply_seen_dedup(&big_output(), "search", &other_args);
        }

        let result = apply_seen_dedup(&output, "get_doc_context", &args);
        // Should be stale (>6 calls gap) so returns full
        assert!(!result.starts_with("(seen"));
    }

    #[test]
    fn test_dedup_skips_small_output() {
        let _g = setup();
        let small = "short output";
        let args = json!({"symbol_id": "src/lib.rs::foo"});

        apply_seen_dedup(small, "get_doc_context", &args);
        let second = apply_seen_dedup(small, "get_doc_context", &args);
        assert_eq!(second, small); // Not deduped — too small
    }

    #[test]
    fn test_dedup_skips_errors() {
        let _g = setup();
        let err = &format!("Error: not found {}", big_output());
        let args = json!({"symbol_id": "src/lib.rs::foo"});

        apply_seen_dedup(err, "get_doc_context", &args);
        let second = apply_seen_dedup(err, "get_doc_context", &args);
        assert!(second.starts_with("Error:"));
    }

    #[test]
    fn test_dedup_disabled_without_env() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_session();
        std::env::remove_var("INFIGRAPH_DEDUP");
        let output = big_output();
        let args = json!({"symbol_id": "src/lib.rs::foo"});

        apply_seen_dedup(&output, "get_doc_context", &args);
        let second = apply_seen_dedup(&output, "get_doc_context", &args);
        assert_eq!(second, output); // No dedup
    }

    #[test]
    fn test_dedup_different_tools_different_keys() {
        let _g = setup();
        let output = big_output();
        let args = json!({"symbol_id": "src/lib.rs::foo"});

        apply_seen_dedup(&output, "get_doc_context", &args);
        let second = apply_seen_dedup(&output, "search", &args);
        assert!(!second.starts_with("(seen")); // Different tool = different key
    }

    #[test]
    fn test_dedup_refreshes_on_hit() {
        let _g = setup();
        let output = big_output();
        let args = json!({"symbol_id": "src/lib.rs::foo"});

        // Call 1: first see
        apply_seen_dedup(&output, "get_doc_context", &args);

        // Calls 2-4: other stuff
        for i in 0..3 {
            let other_args = json!({"symbol_id": format!("other_{i}")});
            apply_seen_dedup(&big_output(), "search", &other_args);
        }

        // Call 5: re-see foo — dedup hit, refreshes counter
        let result = apply_seen_dedup(&output, "get_doc_context", &args);
        assert!(result.starts_with("(seen"));

        // Calls 6-9: more other stuff (4 more)
        for i in 3..7 {
            let other_args = json!({"symbol_id": format!("other_{i}")});
            apply_seen_dedup(&big_output(), "search", &other_args);
        }

        // Call 10: foo again — should still be fresh (refreshed at call 5)
        let result2 = apply_seen_dedup(&output, "get_doc_context", &args);
        assert!(result2.starts_with("(seen"));
    }

    // --- Phase 6: Budget-aware level tests ---

    #[test]
    fn test_auto_level_high_budget() {
        let _g = setup();
        // Fresh session = 0 tokens sent, 150k budget = 100% remaining = Off
        assert_eq!(get_compression_level(), CompressionLevel::Off);
    }

    #[test]
    fn test_auto_level_transitions() {
        let _g = setup();
        std::env::set_var("INFIGRAPH_TOKEN_BUDGET", "100000");
        reset_session();

        // 0% used → Off
        assert_eq!(get_compression_level(), CompressionLevel::Off);

        // Use 35k → 65% remaining → Summary
        let level = track_tokens(35000);
        assert_eq!(level, CompressionLevel::Summary);

        // Use another 25k → 40% remaining → Aggressive
        let level = track_tokens(25000);
        assert_eq!(level, CompressionLevel::Aggressive);

        // Use another 25k → 15% remaining → Minimal
        let level = track_tokens(25000);
        assert_eq!(level, CompressionLevel::Minimal);
    }

    #[test]
    fn test_auto_level_custom_budget() {
        let _g = setup();
        std::env::set_var("INFIGRAPH_TOKEN_BUDGET", "1000");
        reset_session();

        // 900 tokens → 10% remaining → Minimal
        let level = track_tokens(900);
        assert_eq!(level, CompressionLevel::Minimal);
    }

    #[test]
    fn test_track_tokens_cumulative() {
        let _g = setup();
        std::env::set_var("INFIGRAPH_TOKEN_BUDGET", "10000");
        reset_session();

        track_tokens(1000);
        track_tokens(1000);
        track_tokens(1000);
        // 3000/10000 = 70% remaining → Summary (boundary)
        let level = get_compression_level();
        assert_eq!(level, CompressionLevel::Summary);
    }
}
