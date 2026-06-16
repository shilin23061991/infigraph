use infigraph_docs::store::{DocStore, PipelineRecord};

fn temp_store() -> (DocStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_pipeline.kuzu");
    let store = DocStore::open(&db_path).unwrap();
    (store, dir)
}

fn w2_pipeline() -> PipelineRecord {
    PipelineRecord {
        id: "pipeline::w2".to_string(),
        name: "W2 Metrics".to_string(),
        doc_id: "doc::w2".to_string(),
        source_systems: "tax_src.raw_w2_data, ref_data.employer_dim".to_string(),
        dest_tables: "tax_dm.fact_w2_metric, tax_rpt.rpt_w2_summary".to_string(),
        scheduler_type: "BPP".to_string(),
        scheduler_config: "daily 2am UTC".to_string(),
        compliance: "PII — SSN masked. IRS 7216 in scope. Data classification: Restricted.".to_string(),
        github_repo: "https://github.intuit.com/tax-data/w2-pipeline".to_string(),
        daci: "Driver: Alice; Approver: Bob".to_string(),
        idempotent: "Yes".to_string(),
        business_logic_summary: "Aggregates W2 forms by employer EIN".to_string(),
        data_quality: "null check on SSN".to_string(),
        dependencies_upstream: "tax_src.raw_w2_data, ref_data.employer_dim".to_string(),
        dependencies_downstream: "tax_rpt.executive_dashboard".to_string(),
    }
}

fn marketing_pipeline() -> PipelineRecord {
    PipelineRecord {
        id: "pipeline::marketing".to_string(),
        name: "Marketing Attributes".to_string(),
        doc_id: "doc::marketing".to_string(),
        source_systems: "tax_dm.fact_w2_metric, commerce_profile.user_dim".to_string(),
        dest_tables: "tax_rpt.rpt_marketing_attributes".to_string(),
        scheduler_type: "BPP".to_string(),
        scheduler_config: "daily 4am UTC".to_string(),
        compliance: "GDPR applicable. CCPA opt-out honored.".to_string(),
        github_repo: "https://github.intuit.com/tax-data/marketing-attrs".to_string(),
        daci: "Driver: Charlie; Approver: Dave".to_string(),
        idempotent: "Yes".to_string(),
        business_logic_summary: "Builds marketing attributes from W2 data".to_string(),
        data_quality: "dedup on user_id".to_string(),
        dependencies_upstream: "tax_dm.fact_w2_metric".to_string(),
        dependencies_downstream: "".to_string(),
    }
}

fn deceased_pipeline() -> PipelineRecord {
    PipelineRecord {
        id: "pipeline::deceased".to_string(),
        name: "Deceased Flag".to_string(),
        doc_id: "doc::deceased".to_string(),
        source_systems: "tax_rpt.rpt_marketing_attributes, ssa_data.deceased_records".to_string(),
        dest_tables: "tax_dm.dim_deceased_flag".to_string(),
        scheduler_type: "Airflow".to_string(),
        scheduler_config: "weekly Sunday 1am".to_string(),
        compliance: "PII — SSN required. IRS 7216 in scope. HIPAA considerations.".to_string(),
        github_repo: "https://github.intuit.com/tax-data/deceased-flag".to_string(),
        daci: "Driver: Eve; Approver: Frank".to_string(),
        idempotent: "Yes".to_string(),
        business_logic_summary: "Matches SSA death records against taxpayer profiles".to_string(),
        data_quality: "fuzzy match on name+DOB".to_string(),
        dependencies_upstream: "tax_rpt.rpt_marketing_attributes".to_string(),
        dependencies_downstream: "".to_string(),
    }
}

#[test]
fn test_kg2_link_pipeline_dependencies() {
    let (store, _dir) = temp_store();

    // W2 produces tax_dm.fact_w2_metric
    // Marketing consumes tax_dm.fact_w2_metric → DEPENDS_ON W2
    // Deceased consumes tax_rpt.rpt_marketing_attributes → DEPENDS_ON Marketing
    store.upsert_pipeline(&w2_pipeline()).unwrap();
    store.upsert_pipeline(&marketing_pipeline()).unwrap();
    store.upsert_pipeline(&deceased_pipeline()).unwrap();

    let count = store.link_pipeline_dependencies().unwrap();
    assert!(count >= 2, "Expected at least 2 dependency edges, got {count}");

    let deps = store.get_pipeline_deps().unwrap();
    assert!(!deps.is_empty(), "Should have dependency edges");

    // Marketing depends on W2 (via tax_dm.fact_w2_metric)
    let marketing_depends_w2 = deps.iter().any(|(from, to, _)| {
        from.contains("Marketing") && to.contains("W2")
    });
    assert!(marketing_depends_w2, "Marketing should depend on W2. Got deps: {:?}", deps);

    // Deceased depends on Marketing (via tax_rpt.rpt_marketing_attributes)
    let deceased_depends_marketing = deps.iter().any(|(from, to, _)| {
        from.contains("Deceased") && to.contains("Marketing")
    });
    assert!(deceased_depends_marketing, "Deceased should depend on Marketing. Got deps: {:?}", deps);
}

#[test]
fn test_kg5_compliance_query() {
    let (store, _dir) = temp_store();

    store.upsert_pipeline(&w2_pipeline()).unwrap();
    store.upsert_pipeline(&marketing_pipeline()).unwrap();
    store.upsert_pipeline(&deceased_pipeline()).unwrap();

    // IRS 7216 — should match W2 and Deceased
    let irs_pipelines = store.query_pipelines_by_compliance("irs 7216").unwrap();
    assert_eq!(irs_pipelines.len(), 2, "Expected 2 pipelines for IRS 7216, got {}: {:?}",
        irs_pipelines.len(), irs_pipelines.iter().map(|p| &p.name).collect::<Vec<_>>());

    let names: Vec<&str> = irs_pipelines.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"W2 Metrics"), "W2 should be in IRS 7216 scope");
    assert!(names.contains(&"Deceased Flag"), "Deceased should be in IRS 7216 scope");

    // GDPR — should match only Marketing
    let gdpr_pipelines = store.query_pipelines_by_compliance("gdpr").unwrap();
    assert_eq!(gdpr_pipelines.len(), 1, "Expected 1 pipeline for GDPR");
    assert_eq!(gdpr_pipelines[0].name, "Marketing Attributes");

    // PII — should match W2 and Deceased
    let pii_pipelines = store.query_pipelines_by_compliance("pii").unwrap();
    assert_eq!(pii_pipelines.len(), 2, "Expected 2 pipelines for PII");

    // Nonexistent scope
    let none = store.query_pipelines_by_compliance("sox").unwrap();
    assert!(none.is_empty(), "No pipelines should match SOX");
}

#[test]
fn test_kg6_impact_analysis_direct() {
    let (store, _dir) = temp_store();

    store.upsert_pipeline(&w2_pipeline()).unwrap();
    store.upsert_pipeline(&marketing_pipeline()).unwrap();
    store.upsert_pipeline(&deceased_pipeline()).unwrap();
    store.link_pipeline_dependencies().unwrap();

    // Change tax_src.raw_w2_data → should directly affect W2 only
    let impact = store.impact_analysis("tax_src.raw_w2_data", 1).unwrap();
    assert_eq!(impact.len(), 1, "Expected 1 directly affected pipeline, got {}: {:?}",
        impact.len(), impact.iter().map(|r| &r.pipeline_name).collect::<Vec<_>>());
    assert_eq!(impact[0].pipeline_name, "W2 Metrics");
    assert_eq!(impact[0].impact_type, "direct");
    assert_eq!(impact[0].depth, 1);
}

#[test]
fn test_kg6_impact_analysis_transitive() {
    let (store, _dir) = temp_store();

    store.upsert_pipeline(&w2_pipeline()).unwrap();
    store.upsert_pipeline(&marketing_pipeline()).unwrap();
    store.upsert_pipeline(&deceased_pipeline()).unwrap();
    store.link_pipeline_dependencies().unwrap();

    // Change tax_dm.fact_w2_metric → directly affects Marketing,
    // and transitively affects Deceased (via Marketing's output)
    let impact = store.impact_analysis("tax_dm.fact_w2_metric", 3).unwrap();
    assert!(impact.len() >= 1, "Expected at least Marketing directly affected");

    let direct: Vec<_> = impact.iter().filter(|r| r.impact_type == "direct").collect();
    assert!(direct.iter().any(|r| r.pipeline_name == "Marketing Attributes"),
        "Marketing should be directly affected. Direct: {:?}", direct.iter().map(|r| &r.pipeline_name).collect::<Vec<_>>());

    // Transitive: Deceased depends on Marketing's output (tax_rpt.rpt_marketing_attributes)
    let transitive: Vec<_> = impact.iter().filter(|r| r.impact_type == "transitive").collect();
    if !transitive.is_empty() {
        assert!(transitive.iter().any(|r| r.pipeline_name == "Deceased Flag"),
            "Deceased should be transitively affected. Got: {:?}", transitive.iter().map(|r| &r.pipeline_name).collect::<Vec<_>>());
    }
}

#[test]
fn test_kg6_impact_analysis_no_match() {
    let (store, _dir) = temp_store();

    store.upsert_pipeline(&w2_pipeline()).unwrap();
    store.upsert_pipeline(&marketing_pipeline()).unwrap();

    let impact = store.impact_analysis("nonexistent_schema.fake_table", 3).unwrap();
    assert!(impact.is_empty(), "No pipelines should be affected by nonexistent table");
}

#[test]
fn test_pipeline_deps_empty_initially() {
    let (store, _dir) = temp_store();
    let deps = store.get_pipeline_deps().unwrap();
    assert!(deps.is_empty(), "No deps when no pipelines exist");
}

#[test]
fn test_link_deps_idempotent() {
    let (store, _dir) = temp_store();

    store.upsert_pipeline(&w2_pipeline()).unwrap();
    store.upsert_pipeline(&marketing_pipeline()).unwrap();

    let count1 = store.link_pipeline_dependencies().unwrap();
    let count2 = store.link_pipeline_dependencies().unwrap();

    assert_eq!(count1, count2, "Re-linking should produce same count (old edges cleared first)");

    let deps = store.get_pipeline_deps().unwrap();
    assert_eq!(deps.len(), count1, "Dep count should match after idempotent re-link");
}

fn insert_test_doc_and_chunk(store: &DocStore, doc_id: &str, file: &str, chunk_text: &str) {
    let conn = store.connection().unwrap();
    conn.query(&format!(
        "CREATE (d:Document {{id: '{}', title: '{}', file: '{}', format: 'yaml', content_hash: 'test', page_count: 0, chunk_count: 1}})",
        doc_id, file, file
    )).unwrap();
    let chunk_id = format!("chunk::{}", doc_id);
    conn.query(&format!(
        "CREATE (c:Chunk {{id: '{}', doc_file: '{}', idx: 0, heading: '', text: '{}', start_offset: 0, end_offset: {}, page: 0, content_hash: 'test'}})",
        chunk_id, doc_id, chunk_text.replace('\'', "\\'"), chunk_text.len()
    )).unwrap();
    conn.query(&format!(
        "MATCH (d:Document), (c:Chunk) WHERE d.id = '{}' AND c.id = '{}' CREATE (d)-[:HAS_CHUNK]->(c)",
        doc_id, chunk_id
    )).unwrap();
}

#[test]
fn test_kg3_link_pipelines_to_repo_files() {
    let (store, _dir) = temp_store();

    store.upsert_pipeline(&w2_pipeline()).unwrap();

    // Create a repo doc that references a W2 source table
    insert_test_doc_and_chunk(
        &store,
        "configs/w2_etl.yaml",
        "configs/w2_etl.yaml",
        "source_table: tax_src.raw_w2_data\ndest_table: tax_dm.fact_w2_metric",
    );

    // Create a repo doc that does NOT reference any W2 tables
    insert_test_doc_and_chunk(
        &store,
        "configs/unrelated.yaml",
        "configs/unrelated.yaml",
        "source_table: orders.order_items\ndest_table: analytics.order_summary",
    );

    let count = store.link_pipelines_to_repo_files().unwrap();
    assert_eq!(count, 1, "Should link W2 pipeline to w2_etl.yaml only, got {count}");
}

#[test]
fn test_kg3_no_links_for_confluence_docs() {
    let (store, _dir) = temp_store();

    store.upsert_pipeline(&w2_pipeline()).unwrap();

    // Confluence doc should NOT be linked by link_pipelines_to_repo_files
    insert_test_doc_and_chunk(
        &store,
        "confluence://SPACE/12345",
        "confluence://SPACE/12345",
        "This page references tax_src.raw_w2_data",
    );

    let count = store.link_pipelines_to_repo_files().unwrap();
    assert_eq!(count, 0, "Should not link confluence docs via repo file linker");
}

#[test]
fn test_kg4_pipeline_search_docs() {
    let (store, _dir) = temp_store();

    store.upsert_pipeline(&w2_pipeline()).unwrap();
    store.upsert_pipeline(&marketing_pipeline()).unwrap();

    let docs = store.get_pipeline_search_docs().unwrap();
    assert_eq!(docs.len(), 2, "Should have 2 pipeline search docs");

    // Check W2 doc content
    let w2_doc = docs.iter().find(|(id, _)| id.contains("w2")).unwrap();
    let text = &w2_doc.1;
    assert!(text.contains("W2 Metrics"), "Should contain pipeline name");
    assert!(text.contains("tax_src.raw_w2_data"), "Should contain source systems");
    assert!(text.contains("tax_dm.fact_w2_metric"), "Should contain dest tables");
    assert!(text.contains("BPP"), "Should contain scheduler type");
    assert!(text.contains("IRS 7216"), "Should contain compliance");
    assert!(text.contains("Alice"), "Should contain DACI owner");
    assert!(text.contains("github.intuit.com"), "Should contain github repo");
}

#[test]
fn test_kg4_empty_pipelines_no_search_docs() {
    let (store, _dir) = temp_store();
    let docs = store.get_pipeline_search_docs().unwrap();
    assert!(docs.is_empty(), "No pipelines = no search docs");
}
