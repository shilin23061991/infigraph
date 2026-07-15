use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::learned::LearnedStore;
use crate::model::FileExtraction;
use crate::resolve::ResolveStats;

use super::backend::GraphBackend;
use super::queries::GraphQuery;
use super::store::GraphStore;
use super::{
    ApiSymbol, BranchInfo, FileDeps, GraphStats, ImpactRow, ReferenceRow, SymbolDetail, SymbolRow,
    TestContext, TestCoverage, TypeHierarchy,
};

/// Kùzu-backed graph storage (embedded, local mode).
///
/// Wraps existing `GraphStore` + `GraphQuery`. All write methods acquire
/// the write lock internally. Single-writer — concurrent `upsert_files_bulk`
/// calls will serialize on the lock.
pub struct KuzuBackend {
    store: GraphStore,
}

impl KuzuBackend {
    pub fn open(path: &Path) -> Result<Self> {
        let store = GraphStore::open(path)?;
        Ok(Self { store })
    }

    pub fn open_read_only(path: &Path) -> Result<Self> {
        let store = GraphStore::open_read_only(path)?;
        Ok(Self { store })
    }

    /// Access underlying GraphStore (escape hatch for callers that
    /// still need raw Kùzu access during migration).
    pub fn inner(&self) -> &GraphStore {
        &self.store
    }
}

fn escape(s: &str) -> String {
    s.replace('\'', "\\'")
}

impl GraphBackend for KuzuBackend {
    // ── Lifecycle / metadata ─────────────────────────────────────────

    fn stats(&self) -> Result<GraphStats> {
        self.store.stats()
    }

    fn get_file_hashes(&self) -> Result<HashMap<String, String>> {
        self.store.get_file_hashes()
    }

    fn get_all_symbols(&self) -> Result<Vec<(String, String, String, String)>> {
        self.store.get_all_symbols()
    }

    // ── Read: symbol queries ─────────────────────────────────────────

    fn symbols_in_file(&self, file: &str) -> Result<Vec<SymbolRow>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.symbols_in_file(file)
    }

    fn find_symbol_by_id(&self, id: &str) -> Result<Option<SymbolDetail>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.find_symbol_by_id(id)
    }

    fn symbols_in_range(&self, file: &str, start: u32, end: u32) -> Result<Vec<SymbolDetail>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.symbols_in_range(file, start, end)
    }

    fn skeleton(&self, file: &str) -> Result<String> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.skeleton(file)
    }

    // ── Read: graph traversal ────────────────────────────────────────

    fn callers_of(&self, symbol_id: &str) -> Result<Vec<String>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.callers_of(symbol_id)
    }

    fn callees_of(&self, symbol_id: &str) -> Result<Vec<String>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.callees_of(symbol_id)
    }

    fn branches_of(&self, symbol_id: &str) -> Result<Vec<BranchInfo>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.branches_of(symbol_id)
    }

    fn transitive_impact(&self, id: &str, max_depth: u32) -> Result<Vec<ImpactRow>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.transitive_impact(id, max_depth)
    }

    fn find_all_references(&self, id: &str) -> Result<Vec<ReferenceRow>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.find_all_references(id)
    }

    fn cross_cutting_for(&self, id: &str) -> Result<Vec<(String, String)>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.cross_cutting_for(id)
    }

    // ── Read: aggregate queries ──────────────────────────────────────

    fn get_api_surface(&self) -> Result<Vec<ApiSymbol>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.get_api_surface()
    }

    fn get_file_deps(&self, file: &str) -> Result<FileDeps> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.get_file_deps(file)
    }

    fn get_type_hierarchy(&self, id: &str, max_depth: u32) -> Result<TypeHierarchy> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.get_type_hierarchy(id, max_depth)
    }

    fn get_test_coverage(&self) -> Result<TestCoverage> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.get_test_coverage()
    }

    fn generate_test_context(
        &self,
        file_filter: Option<&str>,
        limit: usize,
        test_type: Option<&str>,
    ) -> Result<TestContext> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.generate_test_context(file_filter, limit, test_type)
    }

    // ── Read: raw query ──────────────────────────────────────────────

    fn raw_query(&self, query: &str) -> Result<Vec<Vec<String>>> {
        let conn = self.store.connection()?;
        let q = GraphQuery::new(&conn);
        q.raw_query(query)
    }

    // ── Write ────────────────────────────────────────────────────────

    fn upsert_file(&self, extraction: &FileExtraction) -> Result<()> {
        self.store.upsert_file(extraction)
    }

    fn upsert_files_bulk(
        &self,
        extractions: &[FileExtraction],
        existing_hashes_empty: bool,
    ) -> Result<()> {
        if extractions.is_empty() {
            return Ok(());
        }

        let _write_lock = self.store.write_lock()?;

        let use_csv = existing_hashes_empty || extractions.len() > 100;

        if use_csv {
            // Parquet bulk path: delete stale → COPY FROM → folders
            if !existing_hashes_empty {
                let conn = self.store.connection()?;
                conn.query("BEGIN TRANSACTION")
                    .context("failed to begin delete transaction")?;
                self.delete_files_data(&conn, extractions)?;
                conn.query("COMMIT")
                    .context("failed to commit delete transaction")?;
            }
            let conn = self.store.connection()?;
            self.store.upsert_all_parquet_conn(&conn, extractions)?;
        } else {
            // Per-file UNWIND path for small incremental updates
            let conn = self.store.connection()?;
            conn.query("BEGIN TRANSACTION")
                .context("failed to begin index transaction")?;
            self.delete_files_data(&conn, extractions)?;
            for extraction in extractions {
                self.store.upsert_file_conn_no_delete(&conn, extraction)?;
            }
            conn.query("COMMIT")
                .context("failed to commit index transaction")?;
        }

        // Upsert folder hierarchy
        let file_paths: Vec<&str> = extractions.iter().map(|e| e.file.as_str()).collect();
        let conn = self.store.connection()?;
        self.store.upsert_folders_bulk_conn(&conn, &file_paths)?;

        Ok(())
    }

    fn remove_file(&self, file: &str) -> Result<()> {
        self.store.remove_file(file)
    }

    fn derive_tested_by_edges(&self) -> Result<usize> {
        self.store.derive_tested_by_edges()
    }

    // ── Resolve ──────────────────────────────────────────────────────

    fn resolve_calls(
        &self,
        extractions: &[FileExtraction],
        learned: Option<&LearnedStore>,
    ) -> Result<ResolveStats> {
        crate::resolve::resolve_calls_incremental(&self.store, extractions, learned)
    }
}

impl KuzuBackend {
    /// Delete all graph data for the given files. Caller manages the transaction.
    fn delete_files_data(
        &self,
        conn: &kuzu::Connection<'_>,
        extractions: &[FileExtraction],
    ) -> Result<()> {
        let file_list: Vec<String> = extractions
            .iter()
            .map(|e| format!("'{}'", escape(&e.file)))
            .collect();
        let files_in = file_list.join(", ");

        let _ = conn.query(&format!(
            "MATCH (f:File)-[:DEFINES]->(s:Symbol)-[:HAS_STATEMENT]->(st:Statement) WHERE f.id IN [{}] DETACH DELETE st",
            files_in
        ));
        let _ = conn.query(&format!(
            "MATCH (s:Symbol) WHERE s.file IN [{}] DETACH DELETE s",
            files_in
        ));
        let _ = conn.query(&format!(
            "MATCH (m:Module) WHERE m.file IN [{}] DETACH DELETE m",
            files_in
        ));
        let _ = conn.query(&format!(
            "MATCH (f:File) WHERE f.id IN [{}] DETACH DELETE f",
            files_in
        ));

        Ok(())
    }
}
