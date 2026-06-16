use infigraph_docs::store::{DocStore, PipelineRecord};
use infigraph_docs::chunk::ChunkStrategy;
use infigraph_docs::extract::DocFormat;
use infigraph_docs::DocIndex;

fn make_pipeline(id: &str, name: &str) -> PipelineRecord {
    PipelineRecord {
        id: id.to_string(),
        name: name.to_string(),
        doc_id: format!("doc::{name}"),
        source_systems: "src_table".to_string(),
        dest_tables: "dest_table".to_string(),
        scheduler_type: "Airflow".to_string(),
        scheduler_config: "daily".to_string(),
        compliance: "CCPA".to_string(),
        github_repo: "https://github.com/test/repo".to_string(),
        daci: "Driver: Alice".to_string(),
        idempotent: "true".to_string(),
        business_logic_summary: "Aggregates data".to_string(),
        data_quality: "null checks".to_string(),
        dependencies_upstream: "".to_string(),
        dependencies_downstream: "".to_string(),
    }
}

fn temp_store() -> (tempfile::TempDir, DocStore) {
    let tmp = tempfile::tempdir().unwrap();
    let store = DocStore::open(&tmp.path().join("docs.kuzu")).unwrap();
    (tmp, store)
}

// ==================== DocStore pipeline methods ====================

#[test]
fn test_get_pipeline_after_upsert() {
    let (_tmp, store) = temp_store();
    let p = make_pipeline("pipeline::etl1", "ETL One");
    store.upsert_pipeline(&p).unwrap();

    let fetched = store.get_pipeline("pipeline::etl1").unwrap();
    assert!(fetched.is_some(), "should find pipeline");
    let f = fetched.unwrap();
    assert_eq!(f.name, "ETL One");
    assert_eq!(f.scheduler_type, "Airflow");
}

#[test]
fn test_get_pipeline_missing() {
    let (_tmp, store) = temp_store();
    let fetched = store.get_pipeline("nonexistent").unwrap();
    assert!(fetched.is_none());
}

#[test]
fn test_get_all_pipelines() {
    let (_tmp, store) = temp_store();
    store.upsert_pipeline(&make_pipeline("pipeline::a", "Alpha")).unwrap();
    store.upsert_pipeline(&make_pipeline("pipeline::b", "Beta")).unwrap();
    store.upsert_pipeline(&make_pipeline("pipeline::c", "Gamma")).unwrap();

    let all = store.get_all_pipelines().unwrap();
    assert_eq!(all.len(), 3);
    let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"Alpha"));
    assert!(names.contains(&"Beta"));
    assert!(names.contains(&"Gamma"));
}

#[test]
fn test_get_all_pipelines_empty() {
    let (_tmp, store) = temp_store();
    let all = store.get_all_pipelines().unwrap();
    assert!(all.is_empty());
}

#[test]
fn test_link_pipeline_to_doc() {
    let (_tmp, store) = temp_store();
    let p = make_pipeline("pipeline::linked", "Linked");
    store.upsert_pipeline(&p).unwrap();

    // Create a document to link to
    let doc = infigraph_docs::extract::ExtractedDoc {
        file: "docs/linked.md".to_string(),
        title: Some("Linked Doc".to_string()),
        content_hash: "hash1".to_string(),
        format: DocFormat::Markdown,
        text: "Some doc content".to_string(),
        page_count: None,
    };
    let chunk = infigraph_docs::chunk::Chunk {
        id: "chunk1".to_string(),
        doc_file: "docs/linked.md".to_string(),
        content_hash: "hash1".to_string(),
        index: 0,
        heading: None,
        text: "Some doc content".to_string(),
        start_offset: 0,
        end_offset: 16,
        page: None,
    };
    store.upsert_all_parquet(&[&doc], &[&chunk]).unwrap();

    // Link should not error
    store.link_pipeline_to_doc("pipeline::linked", "docs/linked.md").unwrap();
}

#[test]
fn test_create_depends_on() {
    let (_tmp, store) = temp_store();
    store.upsert_pipeline(&make_pipeline("pipeline::upstream", "Upstream")).unwrap();
    store.upsert_pipeline(&make_pipeline("pipeline::downstream", "Downstream")).unwrap();

    // Create dependency edge
    store.create_depends_on("pipeline::downstream", "pipeline::upstream", "data").unwrap();
}

#[test]
fn test_upsert_pipeline_overwrites() {
    let (_tmp, store) = temp_store();
    store.upsert_pipeline(&make_pipeline("pipeline::x", "Version1")).unwrap();

    let mut p2 = make_pipeline("pipeline::x", "Version2");
    p2.scheduler_type = "Cron".to_string();
    store.upsert_pipeline(&p2).unwrap();

    let fetched = store.get_pipeline("pipeline::x").unwrap().unwrap();
    assert_eq!(fetched.name, "Version2");
    assert_eq!(fetched.scheduler_type, "Cron");
}

// ==================== ChunkStrategy::for_extension ====================

#[test]
fn test_chunk_strategy_for_markdown() {
    match ChunkStrategy::for_extension("md") {
        ChunkStrategy::HeadingBounded => {}
        other => panic!("expected HeadingBounded for .md, got {:?}", other),
    }
}

#[test]
fn test_chunk_strategy_for_html() {
    match ChunkStrategy::for_extension("html") {
        ChunkStrategy::HeadingBounded => {}
        other => panic!("expected HeadingBounded for .html, got {:?}", other),
    }
}

#[test]
fn test_chunk_strategy_for_unknown() {
    match ChunkStrategy::for_extension("xyz") {
        ChunkStrategy::HeadingBounded => {}
        other => panic!("expected HeadingBounded for unknown ext, got {:?}", other),
    }
}

#[test]
fn test_chunk_strategy_for_xml_variants() {
    for ext in &["xml", "xsl", "xsd", "svg", "plist"] {
        match ChunkStrategy::for_extension(ext) {
            ChunkStrategy::HeadingBounded => {}
            other => panic!("expected HeadingBounded for .{ext}, got {:?}", other),
        }
    }
}

// ==================== DocFormat::as_str ====================

#[test]
fn test_doc_format_as_str_all() {
    assert_eq!(DocFormat::Markdown.as_str(), "markdown");
    assert_eq!(DocFormat::PlainText.as_str(), "text");
    assert_eq!(DocFormat::Rst.as_str(), "rst");
    assert_eq!(DocFormat::Asciidoc.as_str(), "asciidoc");
    assert_eq!(DocFormat::Org.as_str(), "org");
    assert_eq!(DocFormat::Pdf.as_str(), "pdf");
    assert_eq!(DocFormat::Docx.as_str(), "docx");
    assert_eq!(DocFormat::Pptx.as_str(), "pptx");
    assert_eq!(DocFormat::Xlsx.as_str(), "xlsx");
    assert_eq!(DocFormat::Html.as_str(), "html");
    assert_eq!(DocFormat::Rtf.as_str(), "rtf");
    assert_eq!(DocFormat::Xml.as_str(), "xml");
}

// ==================== DocIndex::root ====================

#[test]
fn test_doc_index_root() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = DocIndex::open(tmp.path()).unwrap();
    let expected = tmp.path().canonicalize().unwrap();
    let root_str = idx.root().to_string_lossy().to_string();
    let expected_str = expected.to_string_lossy().to_string();
    assert!(
        root_str.ends_with(expected_str.trim_start_matches("/private")),
        "root {root_str} should match {expected_str}"
    );
}
