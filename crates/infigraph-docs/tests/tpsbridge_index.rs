//! Full document index smoke test on TpsBridge.
//!   cargo test -p infigraph-docs --release index_tpsbridge_documents -- --nocapture
use std::path::Path;
use std::time::Instant;

use infigraph_core::embed::doc_embedder;
use infigraph_docs::DocIndex;

const ROOT: &str = "/Users/mlal/SourceCode/TpsBridge";

#[test]
fn index_tpsbridge_documents() {
    let root = Path::new(ROOT);
    assert!(root.is_dir(), "TpsBridge not found at {ROOT}");

    let embedder = doc_embedder();
    eprintln!("Doc embedder: dim={}", embedder.dimension());

    let mut idx = DocIndex::open(root).expect("open doc index");

    let t = Instant::now();
    let result = idx.reindex().expect("full document reindex");
    eprintln!(
        "Doc index: {} files scanned, {} indexed, {} chunks in {:?}",
        result.total_files,
        result.indexed_files,
        result.total_chunks,
        t.elapsed()
    );

    if let Some(store) = idx.store() {
        let stats = store.stats().expect("doc store stats");
        eprintln!(
            "Doc store: {} documents, {} chunks",
            stats.document_count, stats.chunk_count
        );
    }
}
