// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # Cursor y Selecciones
//!
//! Representa posiciones y rangos seleccionados en el documento.
//! El editor soporta múltiples cursores simultáneos (multi-cursor).

use serde::{Deserialize, Serialize};

/// Posición absoluta en el documento expresada como (línea, columna)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct Posicion {
    /// Número de línea (0-indexado)
    pub linea: usize,
    /// Número de columna en caracteres (0-indexado)
    pub columna: usize,
}

impl Posicion {
    pub fn nueva(linea: usize, columna: usize) -> Self {
        Self { linea, columna }
    }

    pub fn origen() -> Self {
        Self::nueva(0, 0)
    }
}

/// Cursor de edición — puede tener una selección activa o no
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cursor {
    /// Posición actual del cursor (donde está el caret)
    pub posicion: Posicion,

    /// Anchor de selección — si es Some, hay una selección activa.
    /// El rango seleccionado va de anchor a posicion.
    pub anchor: Option<Posicion>,
}

impl Cursor {
    pub fn nuevo(linea: usize, columna: usize) -> Self {
        Self {
            posicion: Posicion::nueva(linea, columna),
            anchor: None,
        }
    }

    pub fn en_origen() -> Self {
        Self::nuevo(0, 0)
    }

    /// Retorna true si hay texto seleccionado
    pub fn tiene_seleccion(&self) -> bool {
        self.anchor.is_some() && self.anchor != Some(self.posicion)
    }

    /// Retorna la selección actual si existe, normalizada (inicio <= fin)
    pub fn seleccion(&self) -> Option<Selection> {
        let anchor = self.anchor?;
        if anchor == self.posicion {
            return None;
        }

        let (inicio, fin) = if anchor <= self.posicion {
            (anchor, self.posicion)
        } else {
            (self.posicion, anchor)
        };

        Some(Selection { inicio, fin })
    }

    /// Inicia una selección desde la posición actual (si no hay una activa)
    pub fn iniciar_seleccion(&mut self) {
        if self.anchor.is_none() {
            self.anchor = Some(self.posicion);
        }
    }

    /// Cancela la selección actual
    pub fn cancelar_seleccion(&mut self) {
        self.anchor = None;
    }

    /// Mueve el cursor y opcionalmente extiende la selección
    pub fn mover_a(&mut self, linea: usize, columna: usize, extender_seleccion: bool) {
        if extender_seleccion {
            self.iniciar_seleccion();
        } else {
            self.cancelar_seleccion();
        }
        self.posicion = Posicion::nueva(linea, columna);
    }
}

/// Rango de selección normalizado (inicio siempre <= fin)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub inicio: Posicion,
    pub fin: Posicion,
}

impl Selection {
    pub fn nueva(
        inicio_linea: usize,
        inicio_col: usize,
        fin_linea: usize,
        fin_col: usize,
    ) -> Self {
        Self {
            inicio: Posicion::nueva(inicio_linea, inicio_col),
            fin: Posicion::nueva(fin_linea, fin_col),
        }
    }

    /// Retorna true si la selección abarca múltiples líneas
    pub fn es_multilinea(&self) -> bool {
        self.inicio.linea != self.fin.linea
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_sin_seleccion() {
        let cursor = Cursor::nuevo(5, 10);
        assert!(!cursor.tiene_seleccion());
        assert!(cursor.seleccion().is_none());
    }

    #[test]
    fn test_cursor_con_seleccion() {
        let mut cursor = Cursor::nuevo(0, 0);
        cursor.iniciar_seleccion();
        cursor.mover_a(0, 5, true);

        assert!(cursor.tiene_seleccion());
        let sel = cursor.seleccion().unwrap();
        assert_eq!(sel.inicio, Posicion::nueva(0, 0));
        assert_eq!(sel.fin, Posicion::nueva(0, 5));
    }

    #[test]
    fn test_seleccion_normalizada() {
        let mut cursor = Cursor::nuevo(0, 10);
        cursor.iniciar_seleccion();
        cursor.mover_a(0, 2, true);

        let sel = cursor.seleccion().unwrap();
        assert_eq!(sel.inicio, Posicion::nueva(0, 2));
        assert_eq!(sel.fin, Posicion::nueva(0, 10));
    }
}
