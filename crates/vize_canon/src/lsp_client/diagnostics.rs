use super::{
    diagnostics_api::{document_identifier_uri, flatten_file_diagnostics, map_project_diagnostics},
    session::uri_document_identifier,
    utils::convert_diagnostics,
    CorsaProjectClient, DiagnosticFetch, LspDiagnostic,
};
use corsa::runtime::block_on;
use lsp_types::Diagnostic;
use vize_carton::{FxHashMap, String};

impl CorsaProjectClient {
    /// Get cached diagnostics for a URI.
    pub fn get_diagnostics(&self, uri: &str) -> Vec<LspDiagnostic> {
        self.diagnostics
            .get(uri)
            .map(|diagnostics| convert_diagnostics(diagnostics))
            .unwrap_or_default()
    }

    /// Request diagnostics for a single document.
    pub fn request_diagnostics(&mut self, uri: &str) -> Result<Vec<LspDiagnostic>, String> {
        self.request_diagnostics_full(uri)
            .map(|fetch| convert_diagnostics(&fetch.diagnostics))
    }

    /// Request diagnostics for multiple URIs in batch.
    pub fn request_diagnostics_batch(
        &mut self,
        uris: &[String],
    ) -> Vec<(String, Vec<LspDiagnostic>)> {
        if self.has_materialized_documents(uris) {
            if let Some(results) = self.request_diagnostics_batch_via_materialized_files(uris) {
                return results;
            }
        }

        if self.can_batch_with_project_diagnostics(uris) {
            if let Some(results) = self.request_diagnostics_batch_via_project_api(uris) {
                return results;
            }
        }

        uris.iter()
            .map(|uri| {
                let diagnostics = self.request_diagnostics(uri.as_str()).unwrap_or_default();
                (uri.clone(), diagnostics)
            })
            .collect()
    }

    pub(crate) fn request_diagnostics_full(
        &mut self,
        uri: &str,
    ) -> Result<DiagnosticFetch, String> {
        if self.supports_file_diagnostics_api() && self.can_use_api_for_uri(uri) {
            if let Some(fetch) = self.request_diagnostics_full_via_file_api(uri) {
                return fetch;
            }
        }

        if self.supports_project_diagnostics_api() && self.can_use_api_for_uri(uri) {
            if let Some(fetch) = self.request_diagnostics_full_via_project_api(uri) {
                return fetch;
            }
        }

        Ok(DiagnosticFetch {
            diagnostics: self.cached_diagnostics(uri),
            used_cache: true,
        })
    }

    fn request_diagnostics_batch_via_project_api(
        &mut self,
        uris: &[String],
    ) -> Option<Vec<(String, Vec<LspDiagnostic>)>> {
        let report = block_on(self.session.get_diagnostics_for_project()).ok()?;
        Some(map_project_diagnostics(self, &report, uris))
    }

    fn can_batch_with_project_diagnostics(&self, uris: &[String]) -> bool {
        self.supports_project_diagnostics_api()
            && uris.iter().all(|uri| {
                let uri = uri.as_str();
                self.can_use_api_for_uri(uri) && !self.document_texts.contains_key(uri)
            })
    }

    fn request_diagnostics_full_via_file_api(
        &mut self,
        uri: &str,
    ) -> Option<Result<DiagnosticFetch, String>> {
        let document_uri = self.session_document_uri(uri);
        if document_uri != uri {
            return self.request_materialized_diagnostics_full(uri, document_uri.as_str());
        }

        let response = block_on(
            self.session
                .get_diagnostics_for_file(uri_document_identifier(document_uri.as_str())),
        )
        .ok()?;
        Some(Ok(self.store_file_diagnostics(
            uri,
            self.remap_diagnostics(flatten_file_diagnostics(&response)),
        )))
    }

    fn request_diagnostics_full_via_project_api(
        &mut self,
        uri: &str,
    ) -> Option<Result<DiagnosticFetch, String>> {
        let report = block_on(self.session.get_diagnostics_for_project()).ok()?;
        let diagnostics = report
            .files
            .iter()
            .find(|file| document_identifier_uri(&file.file) == uri)
            .map(flatten_file_diagnostics)
            .unwrap_or_default();
        Some(Ok(self.store_file_diagnostics(
            uri,
            self.remap_diagnostics(diagnostics),
        )))
    }

    fn store_file_diagnostics(
        &mut self,
        uri: &str,
        diagnostics: Vec<Diagnostic>,
    ) -> DiagnosticFetch {
        self.diagnostics.insert(uri.into(), diagnostics.clone());
        DiagnosticFetch {
            diagnostics,
            used_cache: false,
        }
    }

    pub(super) fn cached_diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        self.diagnostics.get(uri).cloned().unwrap_or_default()
    }

    fn has_materialized_documents(&mut self, uris: &[String]) -> bool {
        uris.iter()
            .any(|uri| self.session_document_uri(uri.as_str()) != uri.as_str())
    }

    fn request_materialized_diagnostics_full(
        &mut self,
        external_uri: &str,
        document_uri: &str,
    ) -> Option<Result<DiagnosticFetch, String>> {
        let diagnostics = self.request_materialized_diagnostics(document_uri)?;
        Some(Ok(self.store_file_diagnostics(external_uri, diagnostics)))
    }

    fn request_diagnostics_batch_via_materialized_files(
        &mut self,
        uris: &[String],
    ) -> Option<Vec<(String, Vec<LspDiagnostic>)>> {
        let mut pairs = Vec::with_capacity(uris.len());
        for uri in uris {
            pairs.push((uri.clone(), self.session_document_uri(uri.as_str())));
        }

        if self.supports_project_diagnostics_api() {
            return self.request_materialized_diagnostics_batch_via_project_api(&pairs);
        }

        Some(
            pairs
                .into_iter()
                .map(|(uri, document_uri)| {
                    let diagnostics = self
                        .request_materialized_diagnostics(document_uri.as_str())
                        .unwrap_or_default();
                    self.diagnostics.insert(uri.clone(), diagnostics.clone());
                    (uri, convert_diagnostics(&diagnostics))
                })
                .collect(),
        )
    }

    fn request_materialized_diagnostics(&mut self, document_uri: &str) -> Option<Vec<Diagnostic>> {
        if self.supports_file_diagnostics_api() {
            let response = block_on(
                self.session
                    .get_diagnostics_for_file(uri_document_identifier(document_uri)),
            )
            .ok()?;
            return Some(self.remap_diagnostics(flatten_file_diagnostics(&response)));
        }

        if self.supports_project_diagnostics_api() {
            let report = block_on(self.session.get_diagnostics_for_project()).ok()?;
            let diagnostics = report
                .files
                .iter()
                .find(|file| document_identifier_uri(&file.file) == document_uri)
                .map(flatten_file_diagnostics)
                .unwrap_or_default();
            return Some(self.remap_diagnostics(diagnostics));
        }

        None
    }

    fn request_materialized_diagnostics_batch_via_project_api(
        &mut self,
        pairs: &[(String, String)],
    ) -> Option<Vec<(String, Vec<LspDiagnostic>)>> {
        let report = block_on(self.session.get_diagnostics_for_project()).ok()?;
        let mut diagnostics_by_document: FxHashMap<_, _> = report
            .files
            .iter()
            .map(|file| {
                (
                    document_identifier_uri(&file.file),
                    self.remap_diagnostics(flatten_file_diagnostics(file)),
                )
            })
            .collect();

        Some(
            pairs
                .iter()
                .map(|(uri, document_uri)| {
                    let diagnostics = diagnostics_by_document
                        .remove(document_uri.as_str())
                        .unwrap_or_default();
                    self.diagnostics.insert(uri.clone(), diagnostics.clone());
                    (uri.clone(), convert_diagnostics(&diagnostics))
                })
                .collect(),
        )
    }
}
