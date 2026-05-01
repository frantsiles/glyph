// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # EventoEditor
//!
//! Contrato de eventos entre el renderer (capa de entrada) y la app (capa de lógica).
//!
//! El renderer detecta input del teclado y lo traduce a `EventoEditor`.
//! La app recibe esos eventos, actualiza el `Document` del core y devuelve
//! un `ContenidoRender` actualizado. El renderer no sabe nada de `glyph-core`.

/// Dirección de movimiento del cursor
#[derive(Debug, Clone, Copy)]
pub enum DireccionCursor {
    Izquierda,
    Derecha,
    Arriba,
    Abajo,
    InicioLinea, // Home
    FinLinea,    // End
}

/// Eventos de edición emitidos por el renderer hacia la capa de aplicación
#[derive(Debug, Clone)]
pub enum EventoEditor {
    /// Insertar texto en la posición del cursor (incluye "\n" para Enter)
    InsertarTexto(String),

    /// Borrar el carácter antes del cursor (Backspace)
    BorrarAtras,

    /// Borrar el carácter después del cursor (Delete)
    BorrarAdelante,

    /// Mover el cursor en una dirección
    MoverCursor(DireccionCursor),

    /// Deshacer la última operación (Ctrl+Z)
    Deshacer,

    /// Rehacer la operación deshecha (Ctrl+Y)
    Rehacer,

    /// Guardar el archivo actual (Ctrl+S)
    Guardar,
}
