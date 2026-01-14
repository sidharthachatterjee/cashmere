use dashmap::DashMap;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::linter::{lint_source, LintDiagnostic};

pub struct Backend {
    client: Client,
    document_map: Arc<DashMap<String, String>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            document_map: Arc::new(DashMap::new()),
        }
    }

    async fn lint_document(&self, uri: Url, text: String) {
        let file_path = uri.to_string();
        let diagnostics = lint_source(&text, &file_path);

        let lsp_diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(|d| self.convert_diagnostic(d))
            .collect();

        self.client
            .publish_diagnostics(uri, lsp_diagnostics, None)
            .await;
    }

    fn convert_diagnostic(&self, diag: LintDiagnostic) -> Diagnostic {
        // LSP uses 0-based line and column numbers
        let line = (diag.line - 1) as u32;
        let column = (diag.column - 1) as u32;

        Diagnostic {
            range: Range {
                start: Position {
                    line,
                    character: column,
                },
                end: Position {
                    line,
                    character: column + 1,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(diag.rule)),
            source: Some("cashmere".to_string()),
            message: diag.message,
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        }
    }

    fn is_supported_file(&self, uri: &Url) -> bool {
        if let Some(path) = uri.path().split('/').last() {
            let extensions = ["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"];
            return extensions.iter().any(|ext| path.ends_with(ext));
        }
        false
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "cashmere".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Cashmere LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        if !self.is_supported_file(&uri) {
            return;
        }

        let text = params.text_document.text;
        self.document_map.insert(uri.to_string(), text.clone());
        self.lint_document(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if !self.is_supported_file(&uri) {
            return;
        }

        if let Some(change) = params.content_changes.into_iter().next() {
            let text = change.text;
            self.document_map.insert(uri.to_string(), text.clone());
            self.lint_document(uri, text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        if !self.is_supported_file(&uri) {
            return;
        }

        if let Some(text) = self.document_map.get(&uri.to_string()) {
            self.lint_document(uri, text.clone()).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.document_map
            .remove(&params.text_document.uri.to_string());
    }
}

pub async fn run_lsp_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
