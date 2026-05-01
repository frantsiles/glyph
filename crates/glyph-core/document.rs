// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Document
//!
//! Representa un documento completo en el editor.
//! Integra Buffer + Cursores + Historia en una sola entidad cohesiva.
//!
//! Es la unidad que ve el resto del sistema — el plugin system,
//! el renderer y el LSP interactúan con Document, no directamente con Buffer.

use crate::{
    buffer::Buffer,
    cursor::Cursor,
    history::{Historia, Operacion},
};
use anyhow::Result;
use std::path::PathBuf;
use uuid::Uuid;

/// Documento completo con buffer, cursores e historial
#[derive(Debug)]
pub struct Document {
    /// Identificador único del documento
    pub id: Uuid,

    /// Buffer de texto principal
    pub buffer: Buffer,

    /// Lista de cursores activos (multi-cursor support)
    pub cursores: Vec<Cursor>,

    /// Historial de cambios para undo/redo
    pub historia: Historia,
}

impl Document {
    /// Crea un documento nuevo vacío
    pub fn nuevo() -> Self {
        Self {
            id: Uuid::new_v4(),
            buffer: Buffer::vacio(),
            cursores: vec![Cursor::en_origen()],
            historia: Historia::nueva(),
        }
    }

    /// Crea un documento desde contenido y ruta
    pub fn desde_archivo(contenido: &str, ruta: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            buffer: Buffer::con_ruta(contenido, ruta),
            cursores: vec![Cursor::en_origen()],
            historia: Historia::nueva(),
        }
    }

    // ------------------------------------------------------------------
    // Operaciones de edición — pasan por la historia automáticamente
    // ------------------------------------------------------------------

    /// Inserta texto en la posición del cursor principal
    pub fn insertar_en_cursor(&mut self, texto: &str) -> Result<()> {
        let cursor = self.cursor_principal();
        let indice = self.buffer.posicion_a_indice(
            cursor.posicion.linea,
            cursor.posicion.columna,
        )?;

        // Registrar en historia antes de modificar
        self.historia.registrar(Operacion::Insertar {
            indice,
            texto: texto.to_string(),
        });

        self.buffer.insertar(indice, texto);
        Ok(())
    }

    /// Deshace la última operación
    pub fn deshacer(&mut self) -> Result<()> {
        if let Some(operaciones) = self.historia.deshacer() {
            // Revertir operaciones en orden inverso
            for op in operaciones.iter().rev() {
                match op {
                    Operacion::Insertar { indice, texto } => {
                        self.buffer.eliminar(*indice, indice + texto.chars().count());
                    }
                    Operacion::Eliminar { indice, texto } => {
                        self.buffer.insertar(*indice, texto);
                    }
                }
            }
        }
        Ok(())
    }

    /// Rehace la última operación deshecha
    pub fn rehacer(&mut self) -> Result<()> {
        if let Some(operaciones) = self.historia.rehacer() {
            for op in &operaciones {
                match op {
                    Operacion::Insertar { indice, texto } => {
                        self.buffer.insertar(*indice, texto);
                    }
                    Operacion::Eliminar { indice, texto } => {
                        self.buffer.eliminar(*indice, indice + texto.chars().count());
                    }
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Helpers de cursores
    // ------------------------------------------------------------------

    /// Retorna el cursor principal (el primero de la lista)
    pub fn cursor_principal(&self) -> &Cursor {
        &self.cursores[0]
    }

    /// Retorna el cursor principal de forma mutable
    pub fn cursor_principal_mut(&mut self) -> &mut Cursor {
        &mut self.cursores[0]
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::nuevo()
    }
}