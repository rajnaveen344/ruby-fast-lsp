//! Observe the ordinary tower-lsp client channel without performing analysis.
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{sync::watch, task::JoinHandle};
use tower_lsp::{
    jsonrpc::Response,
    lsp_types::{Diagnostic, PublishDiagnosticsParams, Url},
    ClientSocket,
};

pub(super) struct ClientMessages {
    diagnostics: Arc<Mutex<HashMap<Url, Vec<Diagnostic>>>>,
    changed: watch::Sender<u64>,
    reader: JoinHandle<()>,
}

impl ClientMessages {
    pub(super) fn listen(mut socket: ClientSocket) -> Self {
        let diagnostics = Arc::new(Mutex::new(HashMap::new()));
        let captured = diagnostics.clone();
        let (changed, _) = watch::channel(0_u64);
        let published = changed.clone();
        let reader = tokio::spawn(async move {
            while let Some(message) = socket.next().await {
                if message.method() == "textDocument/publishDiagnostics" {
                    let params: PublishDiagnosticsParams = serde_json::from_value(
                        message
                            .params()
                            .expect("diagnostic notification has parameters")
                            .clone(),
                    )
                    .expect("diagnostics must survive ordinary LSP serialization");
                    captured.lock().insert(params.uri, params.diagnostics);
                    published.send_modify(|sequence| {
                        *sequence = sequence
                            .checked_add(1)
                            .expect("diagnostic observation sequence exhausted")
                    });
                }
                // A minimal editor acknowledges server refresh/registration
                // requests. These responses never alter semantic state.
                if let Some(id) = message.id() {
                    socket
                        .send(Response::from_ok(id.clone(), serde_json::Value::Null))
                        .await
                        .expect("test client response channel stays connected");
                }
            }
        });
        Self {
            diagnostics,
            changed,
            reader,
        }
    }

    pub(super) async fn diagnostics(&self, uri: &Url, submitted: &[Diagnostic]) -> Vec<Diagnostic> {
        let mut changed = self.changed.subscribe();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(delivered) = self.diagnostics.lock().get(uri).cloned() {
                    if delivered == submitted {
                        return delivered;
                    }
                }
                changed.changed().await.expect("test diagnostic observer stays connected");
            }
        }).await.unwrap_or_else(|_| panic!(
            "INVARIANT VIOLATED: submitted diagnostics for {uri} were not delivered through the LSP client. This is a bug because production publication must reach the editor, including empty clears. Fix: inspect the queue, sender, and captured transport output; do not recompute diagnostics in the observer."
        ))
    }

    pub(super) fn notification_count(&self) -> u64 {
        *self.changed.borrow()
    }
}

impl Drop for ClientMessages {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

#[tokio::test]
async fn missing_client_notification_cannot_satisfy_an_empty_diagnostic_assertion() {
    let (_service, socket) = tower_lsp::LspService::new(|client| {
        crate::server::RubyLanguageServer::new(client).expect("construct client observer control")
    });
    let messages = ClientMessages::listen(socket);
    let uri = crate::test::harness::fixture_uri("/observer/unpublished.rb");
    // A submitted empty array is only an expectation. Until an actual client
    // message arrives, the observer must stay pending instead of returning it.
    assert!(
        tokio::time::timeout(Duration::from_millis(25), messages.diagnostics(&uri, &[]))
            .await
            .is_err()
    );
    assert_eq!(messages.notification_count(), 0);
}
