// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! Framing LSP JSON-RPC sobre stdin/stdout.
//!
//! El protocolo es:
//! ```text
//! Content-Length: <n>\r\n
//! \r\n
//! <json de n bytes>
//! ```

use anyhow::{anyhow, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

/// Lee un mensaje LSP de un `BufReader` asíncrono.
pub async fn leer_mensaje<R>(reader: &mut R) -> Result<Value>
where
    R: AsyncBufReadExt + Unpin,
{
    // Leer cabeceras hasta línea vacía
    let mut content_length: Option<usize> = None;
    loop {
        let mut linea = String::new();
        let n = reader.read_line(&mut linea).await?;
        if n == 0 {
            return Err(anyhow!("EOF leyendo cabeceras LSP"));
        }
        let linea = linea.trim();
        if linea.is_empty() {
            break;
        }
        if let Some(valor) = linea.strip_prefix("Content-Length: ") {
            content_length = Some(valor.parse()?);
        }
    }

    let len = content_length.ok_or_else(|| anyhow!("Falta Content-Length en mensaje LSP"))?;

    // Leer cuerpo JSON
    let mut cuerpo = vec![0u8; len];
    reader.read_exact(&mut cuerpo).await?;
    Ok(serde_json::from_slice(&cuerpo)?)
}

/// Escribe un mensaje LSP con su cabecera Content-Length.
pub async fn escribir_mensaje<W>(writer: &mut W, valor: &Value) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let json = serde_json::to_string(valor)?;
    let frame = format!("Content-Length: {}\r\n\r\n{}", json.len(), json);
    writer.write_all(frame.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}
