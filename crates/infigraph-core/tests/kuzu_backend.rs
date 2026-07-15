use infigraph_core::graph::{GraphBackend, KuzuBackend};
use infigraph_core::model::{FileExtraction, Relation, RelationKind, Span, Symbol, SymbolKind};

fn span(file: &str, start: u32, end: u32) -> Span {
    Span {
        file: file.to_string(),
        start_line: start,
        start_col: 0,
        end_line: end,
        end_col: 0,
    }
}

fn sym(id: &str, name: &str, kind: SymbolKind, file: &str, start: u32, end: u32) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        span: span(file, start, end),
        signature_hash: format!("hash_{id}"),
        parent: None,
        language: "python".to_string(),
        visibility: Some("public".to_string()),
        docstring: None,
        complexity: 1,
        parameters: None,
        return_type: None,
    }
}

fn rel(src: &str, tgt: &str, kind: RelationKind) -> Relation {
    Relation {
        source_id: src.to_string(),
        target_id: tgt.to_string(),
        kind,
        span: None,
        receiver: None,
    }
}

fn make_backend() -> (tempfile::TempDir, Box<dyn GraphBackend>) {
    let dir = tempfile::TempDir::new().expect("tmpdir");
    let backend = KuzuBackend::open(&dir.path().join("graph")).expect("open");
    (dir, Box::new(backend))
}

fn fixture() -> Vec<FileExtraction> {
    vec![
        FileExtraction {
            file: "src/main.py".to_string(),
            language: "python".to_string(),
            content_hash: "aaa".to_string(),
            symbols: vec![
                sym(
                    "src/main.py::main",
                    "main",
                    SymbolKind::Function,
                    "src/main.py",
                    1,
                    10,
                ),
                sym(
                    "src/main.py::helper",
                    "helper",
                    SymbolKind::Function,
                    "src/main.py",
                    12,
                    20,
                ),
            ],
            relations: vec![
                rel(
                    "src/main.py::main",
                    "src/main.py::helper",
                    RelationKind::Calls,
                ),
                rel(
                    "src/main.py::main",
                    "src/lib.py::process",
                    RelationKind::Calls,
                ),
            ],
            statements: vec![],
        },
        FileExtraction {
            file: "src/lib.py".to_string(),
            language: "python".to_string(),
            content_hash: "bbb".to_string(),
            symbols: vec![sym(
                "src/lib.py::process",
                "process",
                SymbolKind::Function,
                "src/lib.py",
                1,
                15,
            )],
            relations: vec![],
            statements: vec![],
        },
    ]
}

#[test]
fn test_backend_upsert_bulk_and_stats() {
    let (_dir, backend) = make_backend();
    backend
        .upsert_files_bulk(&fixture(), true)
        .expect("bulk upsert");

    let stats = backend.stats().expect("stats");
    assert_eq!(stats.symbols, 3, "expected 3 symbols");
    assert_eq!(stats.files, 2, "expected 2 files");
    assert!(stats.modules >= 2, "expected at least 2 modules");
}

#[test]
fn test_backend_symbols_in_file() {
    let (_dir, backend) = make_backend();
    backend.upsert_files_bulk(&fixture(), true).expect("bulk");

    let syms = backend.symbols_in_file("src/main.py").expect("query");
    assert_eq!(syms.len(), 2);
    assert!(syms.iter().any(|s| s.name == "main"));
    assert!(syms.iter().any(|s| s.name == "helper"));
}

#[test]
fn test_backend_find_symbol_by_id() {
    let (_dir, backend) = make_backend();
    backend.upsert_files_bulk(&fixture(), true).expect("bulk");

    let sym = backend
        .find_symbol_by_id("src/lib.py::process")
        .expect("query");
    assert!(sym.is_some());
    let sym = sym.unwrap();
    assert_eq!(sym.name, "process");
    assert_eq!(sym.file, "src/lib.py");
}

#[test]
fn test_backend_get_file_hashes() {
    let (_dir, backend) = make_backend();
    backend.upsert_files_bulk(&fixture(), true).expect("bulk");

    let hashes = backend.get_file_hashes().expect("hashes");
    assert_eq!(hashes.get("src/main.py").map(|s| s.as_str()), Some("aaa"));
    assert_eq!(hashes.get("src/lib.py").map(|s| s.as_str()), Some("bbb"));
}

#[test]
fn test_backend_resolve_calls() {
    let (_dir, backend) = make_backend();
    let extractions = fixture();
    backend.upsert_files_bulk(&extractions, true).expect("bulk");

    let stats = backend.resolve_calls(&extractions, None).expect("resolve");
    assert!(stats.resolved > 0, "expected some resolved calls");
}

#[test]
fn test_backend_traversal_after_resolve() {
    let (_dir, backend) = make_backend();
    let extractions = fixture();
    backend.upsert_files_bulk(&extractions, true).expect("bulk");
    backend.resolve_calls(&extractions, None).expect("resolve");

    let callees = backend.callees_of("src/main.py::main").expect("callees");
    assert!(
        callees.iter().any(|c| c.contains("helper")),
        "main should call helper, got: {:?}",
        callees
    );

    let callers = backend.callers_of("src/main.py::helper").expect("callers");
    assert!(
        callers.iter().any(|c| c.contains("main")),
        "helper should be called by main, got: {:?}",
        callers
    );

    let callees_cross = backend.callees_of("src/main.py::main").expect("callees");
    assert!(
        callees_cross.iter().any(|c| c.contains("process")),
        "main should call process cross-file, got: {:?}",
        callees_cross
    );
}

#[test]
fn test_backend_remove_file() {
    let (_dir, backend) = make_backend();
    backend.upsert_files_bulk(&fixture(), true).expect("bulk");

    backend.remove_file("src/lib.py").expect("remove");

    let stats = backend.stats().expect("stats");
    assert_eq!(stats.files, 1, "one file should remain after removal");
}

#[test]
fn test_backend_incremental_upsert() {
    let (_dir, backend) = make_backend();
    let extractions = fixture();

    backend
        .upsert_files_bulk(&extractions, true)
        .expect("fresh");
    let stats1 = backend.stats().expect("stats1");

    backend
        .upsert_files_bulk(&extractions, false)
        .expect("incremental");
    let stats2 = backend.stats().expect("stats2");

    assert_eq!(
        stats1.symbols, stats2.symbols,
        "symbol count same after re-upsert"
    );
    assert_eq!(
        stats1.files, stats2.files,
        "file count same after re-upsert"
    );
}

#[test]
fn test_backend_raw_query() {
    let (_dir, backend) = make_backend();
    backend.upsert_files_bulk(&fixture(), true).expect("bulk");

    let rows = backend
        .raw_query("MATCH (s:Symbol) RETURN s.name ORDER BY s.name")
        .expect("raw");
    assert_eq!(rows.len(), 3);
}

#[test]
fn test_backend_skeleton() {
    let (_dir, backend) = make_backend();
    backend.upsert_files_bulk(&fixture(), true).expect("bulk");

    let skel = backend.skeleton("src/main.py").expect("skeleton");
    assert!(skel.contains("main"), "skeleton should contain main");
    assert!(skel.contains("helper"), "skeleton should contain helper");
}

#[test]
fn test_backend_trait_object() {
    fn use_backend(b: &dyn GraphBackend) -> u64 {
        b.stats().expect("stats").symbols
    }

    let (_dir, backend) = make_backend();
    backend.upsert_files_bulk(&fixture(), true).expect("bulk");
    assert_eq!(use_backend(backend.as_ref()), 3);
}
