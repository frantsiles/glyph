// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Document
//!
//! Representa un documento completo en el editor.
//! Integra Buffer + Cursores + Historia en una sola entidad cohesiva.
//!
//! Es la unidad que ve el resto del sistema — el renderer, el LSP y
//! los plugins interactúan con Document, no directamente con Buffer.

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
    // Consultas de cursor
    // ------------------------------------------------------------------

    /// Retorna el cursor principal (el primero de la lista)
    pub fn cursor_principal(&self) -> &Cursor {
        &self.cursores[0]
    }

    /// Retorna el cursor principal de forma mutable
    pub fn cursor_principal_mut(&mut self) -> &mut Cursor {
        &mut self.cursores[0]
    }

    // ------------------------------------------------------------------
    // Inserción
    // ------------------------------------------------------------------

    /// Inserta texto en la posición del cursor principal y avanza el cursor.
    pub fn insertar_en_cursor(&mut self, texto: &str) -> Result<()> {
        let pos = self.cursores[0].posicion; // Copy — el borrow termina aquí
        let indice = self.buffer.posicion_a_indice(pos.linea, pos.columna)?;

        self.historia.registrar(Operacion::Insertar {
            indice,
            texto: texto.to_string(),
        });

        self.buffer.insertar(indice, texto);

        // Avanzar cursor al final del texto insertado
        let nuevo_indice = indice + texto.chars().count();
        if let Ok((nueva_linea, nueva_col)) = self.buffer.indice_a_posicion(nuevo_indice) {
            self.cursores[0].mover_a(nueva_linea, nueva_col, false);
        }

        Ok(())
    }

    // ------------------------------------------------------------------
    // Borrado
    // ------------------------------------------------------------------

    /// Borra el carácter inmediatamente antes del cursor (Backspace).
    pub fn borrar_antes_cursor(&mut self) -> Result<()> {
        let pos = self.cursores[0].posicion; // Copy
        let indice = self.buffer.posicion_a_indice(pos.linea, pos.columna)?;

        if indice == 0 {
            return Ok(()); // ya estamos al inicio del documento
        }

        let inicio = indice - 1;
        let texto_borrado = self.buffer.rango_texto(inicio, indice);

        self.historia.registrar(Operacion::Eliminar {
            indice: inicio,
            texto: texto_borrado,
        });

        self.buffer.eliminar(inicio, indice);

        // Retroceder cursor una posición
        if let Ok((nueva_linea, nueva_col)) = self.buffer.indice_a_posicion(inicio) {
            self.cursores[0].mover_a(nueva_linea, nueva_col, false);
        }

        Ok(())
    }

    /// Borra el carácter inmediatamente después del cursor (Delete).
    pub fn borrar_despues_cursor(&mut self) -> Result<()> {
        let pos = self.cursores[0].posicion; // Copy
        let indice = self.buffer.posicion_a_indice(pos.linea, pos.columna)?;

        if indice >= self.buffer.caracteres() {
            return Ok(()); // ya estamos al final del documento
        }

        let texto_borrado = self.buffer.rango_texto(indice, indice + 1);

        self.historia.registrar(Operacion::Eliminar {
            indice,
            texto: texto_borrado,
        });

        self.buffer.eliminar(indice, indice + 1);
        // El cursor no se mueve — el texto siguiente "sube" hacia él

        Ok(())
    }

    // ------------------------------------------------------------------
    // Undo / Redo
    // ------------------------------------------------------------------

    /// Deshace la última operación
    pub fn deshacer(&mut self) -> Result<()> {
        if let Some(operaciones) = self.historia.deshacer() {
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
    // Movimiento del cursor
    // ------------------------------------------------------------------

    /// Mueve el cursor un carácter a la izquierda.
    /// Si está al inicio de línea, sube al final de la línea anterior.
    pub fn mover_cursor_izquierda(&mut self) {
        let pos = self.cursores[0].posicion; // Copy
        let Ok(indice) = self.buffer.posicion_a_indice(pos.linea, pos.columna) else {
            return;
        };
        if indice == 0 {
            return;
        }
        if let Ok((linea, col)) = self.buffer.indice_a_posicion(indice - 1) {
            self.cursores[0].mover_a(linea, col, false);
        }
    }

    /// Mueve el cursor un carácter a la derecha.
    /// Si está al final de línea, baja al inicio de la siguiente.
    pub fn mover_cursor_derecha(&mut self) {
        let pos = self.cursores[0].posicion; // Copy
        let Ok(indice) = self.buffer.posicion_a_indice(pos.linea, pos.columna) else {
            return;
        };
        if indice >= self.buffer.caracteres() {
            return;
        }
        if let Ok((linea, col)) = self.buffer.indice_a_posicion(indice + 1) {
            self.cursores[0].mover_a(linea, col, false);
        }
    }

    /// Mueve el cursor una línea hacia arriba, conservando la columna si cabe.
    pub fn mover_cursor_arriba(&mut self) {
        let pos = self.cursores[0].posicion; // Copy
        if pos.linea == 0 {
            self.cursores[0].mover_a(0, 0, false);
            return;
        }
        let nueva_linea = pos.linea - 1;
        let max_col = self.longitud_linea(nueva_linea);
        self.cursores[0].mover_a(nueva_linea, pos.columna.min(max_col), false);
    }

    /// Mueve el cursor una línea hacia abajo, conservando la columna si cabe.
    pub fn mover_cursor_abajo(&mut self) {
        let pos = self.cursores[0].posicion; // Copy
        let total = self.buffer.lineas();
        if pos.linea + 1 >= total {
            return;
        }
        let nueva_linea = pos.linea + 1;
        let max_col = self.longitud_linea(nueva_linea);
        self.cursores[0].mover_a(nueva_linea, pos.columna.min(max_col), false);
    }

    /// Mueve el cursor al inicio de la línea actual (Home).
    pub fn mover_cursor_inicio_linea(&mut self) {
        let linea = self.cursores[0].posicion.linea;
        self.cursores[0].mover_a(linea, 0, false);
    }

    /// Mueve el cursor al final de la línea actual (End).
    pub fn mover_cursor_fin_linea(&mut self) {
        let linea = self.cursores[0].posicion.linea;
        let col = self.longitud_linea(linea);
        self.cursores[0].mover_a(linea, col, false);
    }

    /// Mueve el cursor `n` líneas hacia arriba (Page Up).
    pub fn mover_cursor_pagina_arriba(&mut self, n: usize) {
        let pos = self.cursores[0].posicion;
        let nueva_linea = pos.linea.saturating_sub(n);
        let max_col = self.longitud_linea(nueva_linea);
        self.cursores[0].mover_a(nueva_linea, pos.columna.min(max_col), false);
    }

    /// Mueve el cursor `n` líneas hacia abajo (Page Down).
    pub fn mover_cursor_pagina_abajo(&mut self, n: usize) {
        let pos = self.cursores[0].posicion;
        let total = self.buffer.lineas();
        let nueva_linea = (pos.linea + n).min(total.saturating_sub(1));
        let max_col = self.longitud_linea(nueva_linea);
        self.cursores[0].mover_a(nueva_linea, pos.columna.min(max_col), false);
    }

    /// Mueve el cursor al inicio del documento (Ctrl+Home).
    pub fn mover_cursor_inicio_doc(&mut self) {
        self.cursores[0].mover_a(0, 0, false);
    }

    /// Mueve el cursor al final del documento (Ctrl+End).
    pub fn mover_cursor_fin_doc(&mut self) {
        let total = self.buffer.lineas();
        if total == 0 {
            return;
        }
        let ultima = total - 1;
        let col = self.longitud_linea(ultima);
        self.cursores[0].mover_a(ultima, col, false);
    }

    // ------------------------------------------------------------------
    // Búsqueda
    // ------------------------------------------------------------------

    /// Busca todas las ocurrencias (no solapadas) de `consulta` en el buffer.
    /// Devuelve rangos de bytes `(inicio, fin)` en el texto completo.
    pub fn buscar(&self, consulta: &str) -> Vec<(usize, usize)> {
        if consulta.is_empty() {
            return vec![];
        }
        let texto = self.buffer.contenido_completo();
        let mut matches = Vec::new();
        let mut pos = 0;
        while let Some(idx) = texto[pos..].find(consulta) {
            let inicio = pos + idx;
            let fin = inicio + consulta.len();
            matches.push((inicio, fin));
            pos = fin;
        }
        matches
    }

    /// Mueve el cursor principal a un byte offset del texto completo.
    pub fn mover_cursor_a_byte(&mut self, byte: usize) {
        let (linea, col) = self.buffer.byte_a_posicion(byte);
        self.cursores[0].mover_a(linea, col, false);
    }

    /// Mueve el cursor a (línea, columna) en coordenadas de carácter.
    /// Satura al rango válido del documento.
    pub fn mover_cursor_a(&mut self, linea: usize, columna: usize) {
        let total = self.buffer.lineas();
        let linea = linea.min(total.saturating_sub(1));
        let max_col = self.longitud_linea(linea);
        self.cursores[0].mover_a(linea, columna.min(max_col), false);
    }

    /// Reemplaza el rango de bytes `[inicio_byte, fin_byte)` con `nuevo`.
    /// Registra la operación como un grupo de undo único.
    pub fn reemplazar_bytes(&mut self, inicio_byte: usize, fin_byte: usize, nuevo: &str) -> Result<()> {
        let ini = self.buffer.byte_a_char_idx(inicio_byte);
        let fin = self.buffer.byte_a_char_idx(fin_byte);

        let original = self.buffer.rango_texto(ini, fin);
        self.historia.registrar(Operacion::Eliminar { indice: ini, texto: original });
        self.historia.registrar(Operacion::Insertar { indice: ini, texto: nuevo.to_string() });
        self.historia.confirmar_grupo();

        self.buffer.reemplazar(ini, fin, nuevo);

        let nuevo_fin = ini + nuevo.chars().count();
        if let Ok((l, c)) = self.buffer.indice_a_posicion(nuevo_fin) {
            self.cursores[0].mover_a(l, c, false);
        }
        Ok(())
    }

    /// Reemplaza todas las ocurrencias en `matches` con `nuevo`, procesando
    /// de derecha a izquierda para que los byte offsets se mantengan válidos.
    pub fn reemplazar_todo_bytes(&mut self, matches: &[(usize, usize)], nuevo: &str) -> Result<()> {
        if matches.is_empty() {
            return Ok(());
        }
        // Orden inverso: mayor offset primero para no invalidar los anteriores
        let mut ordenados: Vec<(usize, usize)> = matches.to_vec();
        ordenados.sort_by(|a, b| b.0.cmp(&a.0));

        for (ini_b, fin_b) in &ordenados {
            let ini = self.buffer.byte_a_char_idx(*ini_b);
            let fin = self.buffer.byte_a_char_idx(*fin_b);
            let original = self.buffer.rango_texto(ini, fin);
            self.historia.registrar(Operacion::Eliminar { indice: ini, texto: original });
            self.historia.registrar(Operacion::Insertar { indice: ini, texto: nuevo.to_string() });
            self.buffer.reemplazar(ini, fin, nuevo);
        }
        self.historia.confirmar_grupo();
        Ok(())
    }

    // ------------------------------------------------------------------
    // Helpers privados
    // ------------------------------------------------------------------

    /// Longitud en caracteres de una línea, sin contar el \n final.
    fn longitud_linea(&self, linea: usize) -> usize {
        self.buffer
            .linea(linea)
            .map(|l| l.trim_end_matches('\n').chars().count())
            .unwrap_or(0)
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::nuevo()
    }
}

// ------------------------------------------------------------------
// Tests
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insertar_actualiza_cursor() {
        let mut doc = Document::nuevo();
        doc.insertar_en_cursor("hola").unwrap();
        let pos = doc.cursor_principal().posicion;
        assert_eq!(pos.columna, 4);
        assert_eq!(pos.linea, 0);
    }

    #[test]
    fn test_insertar_newline_mueve_linea() {
        let mut doc = Document::nuevo();
        doc.insertar_en_cursor("ab\ncd").unwrap();
        let pos = doc.cursor_principal().posicion;
        assert_eq!(pos.linea, 1);
        assert_eq!(pos.columna, 2);
    }

    #[test]
    fn test_borrar_antes_cursor() {
        let mut doc = Document::nuevo();
        doc.insertar_en_cursor("hola").unwrap();
        doc.borrar_antes_cursor().unwrap();
        assert_eq!(doc.buffer.contenido_completo(), "hol");
        assert_eq!(doc.cursor_principal().posicion.columna, 3);
    }

    #[test]
    fn test_borrar_despues_cursor() {
        let mut doc = Document::desde_archivo("hola", PathBuf::from("f.txt"));
        doc.borrar_despues_cursor().unwrap();
        assert_eq!(doc.buffer.contenido_completo(), "ola");
        assert_eq!(doc.cursor_principal().posicion.columna, 0);
    }

    #[test]
    fn test_mover_cursor_izquierda_derecha() {
        let mut doc = Document::desde_archivo("abc", PathBuf::from("f.txt"));
        doc.mover_cursor_derecha();
        assert_eq!(doc.cursor_principal().posicion.columna, 1);
        doc.mover_cursor_izquierda();
        assert_eq!(doc.cursor_principal().posicion.columna, 0);
    }

    #[test]
    fn test_mover_cursor_arriba_abajo() {
        let mut doc = Document::desde_archivo("linea1\nlinea2", PathBuf::from("f.txt"));
        doc.mover_cursor_abajo();
        assert_eq!(doc.cursor_principal().posicion.linea, 1);
        doc.mover_cursor_arriba();
        assert_eq!(doc.cursor_principal().posicion.linea, 0);
    }

    #[test]
    fn test_inicio_fin_linea() {
        let mut doc = Document::desde_archivo("hola mundo", PathBuf::from("f.txt"));
        doc.mover_cursor_fin_linea();
        assert_eq!(doc.cursor_principal().posicion.columna, 10);
        doc.mover_cursor_inicio_linea();
        assert_eq!(doc.cursor_principal().posicion.columna, 0);
    }

    #[test]
    fn test_buscar_multiples_ocurrencias() {
        let doc = Document::desde_archivo("fn foo() { let foo = 1; }", PathBuf::from("f.rs"));
        let matches = doc.buscar("foo");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], (3, 6));
        assert_eq!(matches[1], (15, 18));
    }

    #[test]
    fn test_buscar_sin_resultados() {
        let doc = Document::desde_archivo("hola mundo", PathBuf::from("f.txt"));
        assert!(doc.buscar("xyz").is_empty());
        assert!(doc.buscar("").is_empty());
    }

    #[test]
    fn test_mover_cursor_a_byte() {
        let mut doc = Document::desde_archivo("ab\ncd\nef", PathBuf::from("f.txt"));
        doc.mover_cursor_a_byte(4); // 'c' en línea 1, col 1
        let pos = doc.cursor_principal().posicion;
        assert_eq!(pos.linea, 1);
        assert_eq!(pos.columna, 1);
    }
}
