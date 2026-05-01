// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # ClienteLsp
//!
//! Cliente JSON-RPC asíncrono para servidores LSP.
//!
//! ## Ciclo de vida
//!
//! ```text
//! ClienteLsp::conectar(cmd, args, raiz)
//!   ├─ tokio::process::Command::spawn  →  proceso hijo
//!   ├─ tarea_escritura  →  stdin del proceso
//!   ├─ tarea_lectura    ←  stdout del proceso
//!   └─ initialize / initialized handshake
//! ```
//!
//! Después de `conectar`, llamar a `abrir_documento` y `cambiar_documento`
//! según el flujo del editor. Las notificaciones entrantes (diagnósticos,
//! etc.) llegan a través de `rx_notificacion`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use lsp_types::{
    ClientCapabilities, ClientInfo, HoverContents, MarkedString,
    PublishDiagnosticsParams, TextDocumentClientCapabilities,
    HoverClientCapabilities, MarkupKind, Position, Url,
};
use serde_json::Value;
use tokio::io::BufReader;
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};

use crate::transporte;

// ------------------------------------------------------------------
// Tipos públicos
// ------------------------------------------------------------------

/// Notificaciones enviadas por el servidor LSP (sin ID de petición).
#[derive(Debug)]
pub enum Notificacion {
    Diagnosticos(PublishDiagnosticsParams),
}

// ------------------------------------------------------------------
// ClienteLsp
// ------------------------------------------------------------------

/// Cliente LSP asíncrono. Debe vivir dentro de un runtime Tokio.
///
/// `rx_notificacion` recibe las notificaciones del servidor (diagnósticos,
/// etc.) y puede usarse directamente en un `tokio::select!`.
pub struct ClienteLsp {
    tx_escritura: mpsc::UnboundedSender<Value>,
    pub rx_notificacion: mpsc::UnboundedReceiver<Notificacion>,
    pendientes: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    prox_id: Arc<AtomicU64>,
}

impl ClienteLsp {
    /// Conecta a un servidor LSP, realiza el handshake `initialize` y devuelve
    /// el cliente listo para usar.
    ///
    /// `raiz` es la carpeta raíz del workspace que se reporta al servidor.
    pub async fn conectar(cmd: &str, args: &[&str], raiz: &Path) -> Result<Self> {
        let mut proceso = Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| anyhow!("No se pudo lanzar '{cmd}': {e}"))?;

        let stdin = proceso.stdin.take().expect("stdin del proceso LSP");
        let stdout = proceso.stdout.take().expect("stdout del proceso LSP");

        let (tx_escritura, rx_escritura) = mpsc::unbounded_channel::<Value>();
        let (tx_notificacion, rx_notificacion) = mpsc::unbounded_channel::<Notificacion>();
        let pendientes: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        tokio::spawn(tarea_escritura(stdin, rx_escritura));
        tokio::spawn(tarea_lectura(stdout, tx_notificacion, pendientes.clone()));

        let cliente = Self {
            tx_escritura,
            rx_notificacion,
            pendientes,
            prox_id: Arc::new(AtomicU64::new(1)),
        };

        cliente.handshake(raiz).await?;
        Ok(cliente)
    }

    // ── Notificaciones salientes ─────────────────────────────────────

    /// Notifica al servidor que un documento se ha abierto.
    pub async fn abrir_documento(&self, uri: Url, texto: &str, version: i32) -> Result<()> {
        self.notificar(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": lenguaje_de_uri(&uri),
                    "version": version,
                    "text": texto,
                }
            }),
        )
        .await
    }

    /// Notifica al servidor que el contenido de un documento ha cambiado.
    /// Usa sincronización de documento completo (sin rangos incrementales).
    pub async fn cambiar_documento(&self, uri: Url, texto: &str, version: i32) -> Result<()> {
        self.notificar(
            "textDocument/didChange",
            serde_json::json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": texto }],
            }),
        )
        .await
    }

    // ── Peticiones salientes ─────────────────────────────────────────

    /// Pide información de hover en la posición indicada.
    /// Devuelve el texto formateado listo para mostrar, o `None` si no hay hover.
    pub async fn hover(&self, uri: Url, posicion: Position) -> Result<Option<String>> {
        let resultado = self
            .pedir(
                "textDocument/hover",
                serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": posicion.line, "character": posicion.character },
                }),
            )
            .await?;

        if resultado.is_null() {
            return Ok(None);
        }

        let hover: lsp_types::Hover = serde_json::from_value(resultado)
            .map_err(|e| anyhow!("Error deserializando Hover: {e}"))?;

        Ok(hover_a_texto(hover))
    }

    // ── Helpers privados ─────────────────────────────────────────────

    /// Envía una notificación JSON-RPC (sin ID, sin esperar respuesta).
    async fn notificar(&self, method: &str, params: Value) -> Result<()> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.tx_escritura
            .send(msg)
            .map_err(|_| anyhow!("Canal de escritura LSP cerrado"))
    }

    /// Envía una petición JSON-RPC y espera la respuesta.
    async fn pedir(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.prox_id.fetch_add(1, Ordering::Relaxed);

        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let (tx, rx) = oneshot::channel();
        self.pendientes.lock().unwrap().insert(id, tx);

        self.tx_escritura
            .send(msg)
            .map_err(|_| anyhow!("Canal de escritura LSP cerrado"))?;

        rx.await
            .map_err(|_| anyhow!("Respuesta LSP cancelada (servidor cerrado)"))
    }

    /// Handshake `initialize` + `initialized` requerido por el protocolo LSP.
    async fn handshake(&self, raiz: &Path) -> Result<()> {
        let raiz_abs = raiz
            .canonicalize()
            .unwrap_or_else(|_| raiz.to_path_buf());

        let root_uri = Url::from_file_path(&raiz_abs)
            .map_err(|_| anyhow!("Ruta de workspace inválida: {}", raiz_abs.display()))?;

        let caps = ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                hover: Some(HoverClientCapabilities {
                    dynamic_registration: Some(false),
                    content_format: Some(vec![MarkupKind::PlainText, MarkupKind::Markdown]),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let _res = self
            .pedir(
                "initialize",
                serde_json::json!({
                    "processId": std::process::id(),
                    "rootUri": root_uri,
                    "capabilities": caps,
                    "clientInfo": ClientInfo {
                        name: "glyph".to_string(),
                        version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    },
                }),
            )
            .await?;

        self.notificar("initialized", serde_json::json!({})).await?;
        tracing::info!("LSP handshake completado con '{}'", root_uri);
        Ok(())
    }
}

// ------------------------------------------------------------------
// Tareas de fondo
// ------------------------------------------------------------------

async fn tarea_escritura(
    mut stdin: ChildStdin,
    mut rx: mpsc::UnboundedReceiver<Value>,
) {
    while let Some(valor) = rx.recv().await {
        if let Err(e) = transporte::escribir_mensaje(&mut stdin, &valor).await {
            tracing::warn!("Error escribiendo a LSP: {e}");
            break;
        }
    }
}

async fn tarea_lectura(
    stdout: ChildStdout,
    tx_notif: mpsc::UnboundedSender<Notificacion>,
    pendientes: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let valor = match transporte::leer_mensaje(&mut reader).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("LSP reader terminó: {e}");
                break;
            }
        };

        // Respuesta a petición previa (tiene "id" pero no "method")
        if let (Some(id_val), None) = (valor.get("id"), valor.get("method")) {
            if let Some(id) = id_val.as_u64() {
                let resultado = valor
                    .get("result")
                    .cloned()
                    .unwrap_or(Value::Null);
                if let Some(tx) = pendientes.lock().unwrap().remove(&id) {
                    let _ = tx.send(resultado);
                }
            }
            continue;
        }

        // Notificación del servidor (tiene "method" pero no "id")
        if let Some(method) = valor.get("method").and_then(|m| m.as_str()) {
            let params = valor.get("params").cloned().unwrap_or(Value::Null);
            match method {
                "textDocument/publishDiagnostics" => {
                    match serde_json::from_value::<PublishDiagnosticsParams>(params) {
                        Ok(p) => {
                            let _ = tx_notif.send(Notificacion::Diagnosticos(p));
                        }
                        Err(e) => tracing::warn!("Error deserializando diagnósticos: {e}"),
                    }
                }
                // Peticiones del servidor que no necesitan respuesta (window/logMessage, etc.)
                _ => {
                    tracing::debug!("LSP notificación ignorada: {method}");
                }
            }
        }
    }
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

fn hover_a_texto(hover: lsp_types::Hover) -> Option<String> {
    match hover.contents {
        HoverContents::Scalar(MarkedString::String(s)) => Some(s),
        HoverContents::Scalar(MarkedString::LanguageString(ls)) => Some(ls.value),
        HoverContents::Array(arr) => arr.into_iter().find_map(|ms| match ms {
            MarkedString::String(s) => Some(s),
            MarkedString::LanguageString(ls) => Some(ls.value),
        }),
        HoverContents::Markup(mc) => Some(mc.value),
    }
}

fn lenguaje_de_uri(uri: &Url) -> &'static str {
    uri.path()
        .rsplit('.')
        .next()
        .map(|ext| match ext {
            "rs" => "rust",
            "ts" | "tsx" => "typescript",
            "js" | "jsx" => "javascript",
            "py" => "python",
            "go" => "go",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" => "cpp",
            _ => "plaintext",
        })
        .unwrap_or("plaintext")
}
