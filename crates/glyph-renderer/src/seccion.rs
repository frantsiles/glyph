// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # SeccionUI y LayoutManager
//!
//! Sistema de layout para las secciones visuales del editor.
//!
//! ## Algoritmo de layout
//!
//! 1. Las secciones `Arriba` y `Abajo` se resuelven primero (full-width).
//! 2. Las secciones `Izquierda` y `Derecha` se resuelven sobre la franja central.
//! 3. La sección `Centro` ocupa todo el espacio restante.

use crate::quads::{ColorRgba, RectPx};

#[derive(Debug, Clone, PartialEq)]
pub enum LadoLayout {
    Izquierda,
    Derecha,
    Arriba,
    Abajo,
    Centro,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TamanoPref {
    /// Tamaño fijo en píxeles
    Fijo(f32),
    /// Fracción del espacio disponible (0.0–1.0)
    Flex(f32),
    /// Rango mínimo-máximo en píxeles (para el gutter en M8)
    Minmax(f32, f32),
}

#[derive(Debug, Clone)]
pub struct SeccionUI {
    pub id: String,
    pub lado: LadoLayout,
    pub tamano_pref: TamanoPref,
    pub visible: bool,
    pub z_order: i32,
    pub color_fondo: Option<ColorRgba>,
}

#[derive(Debug, Clone)]
pub struct GeometriaSolucion {
    pub id: String,
    pub rect: RectPx,
}

pub struct LayoutManager {
    secciones: Vec<SeccionUI>,
    soluciones: Vec<GeometriaSolucion>,
}

impl LayoutManager {
    pub fn nuevo() -> Self {
        Self { secciones: Vec::new(), soluciones: Vec::new() }
    }

    pub fn registrar(&mut self, seccion: SeccionUI) {
        self.secciones.retain(|s| s.id != seccion.id);
        self.secciones.push(seccion);
        self.secciones.sort_by_key(|s| s.z_order);
    }

    pub fn quitar(&mut self, id: &str) {
        self.secciones.retain(|s| s.id != id);
    }

    pub fn establecer_visible(&mut self, id: &str, visible: bool) {
        if let Some(s) = self.secciones.iter_mut().find(|s| s.id == id) {
            s.visible = visible;
        }
    }

    /// Calcula la geometría de todas las secciones visibles.
    pub fn calcular(&mut self, ancho: f32, alto: f32) -> &[GeometriaSolucion] {
        self.soluciones.clear();

        let mut resto = RectPx { x: 0.0, y: 0.0, ancho, alto };

        // 1. Arriba
        let arriba: Vec<SeccionUI> = self.secciones.iter()
            .filter(|s| s.visible && s.lado == LadoLayout::Arriba)
            .cloned().collect();
        for s in &arriba {
            let h = resolver_tamano(&s.tamano_pref, resto.alto);
            self.soluciones.push(GeometriaSolucion {
                id: s.id.clone(),
                rect: RectPx { x: resto.x, y: resto.y, ancho: resto.ancho, alto: h },
            });
            resto.y += h;
            resto.alto -= h;
        }

        // 2. Abajo
        let abajo: Vec<SeccionUI> = self.secciones.iter()
            .filter(|s| s.visible && s.lado == LadoLayout::Abajo)
            .cloned().collect();
        for s in &abajo {
            let h = resolver_tamano(&s.tamano_pref, resto.alto);
            resto.alto -= h;
            self.soluciones.push(GeometriaSolucion {
                id: s.id.clone(),
                rect: RectPx { x: resto.x, y: resto.y + resto.alto, ancho: resto.ancho, alto: h },
            });
        }

        // 3. Izquierda
        let izquierda: Vec<SeccionUI> = self.secciones.iter()
            .filter(|s| s.visible && s.lado == LadoLayout::Izquierda)
            .cloned().collect();
        for s in &izquierda {
            let w = resolver_tamano(&s.tamano_pref, resto.ancho);
            self.soluciones.push(GeometriaSolucion {
                id: s.id.clone(),
                rect: RectPx { x: resto.x, y: resto.y, ancho: w, alto: resto.alto },
            });
            resto.x += w;
            resto.ancho -= w;
        }

        // 4. Derecha
        let derecha: Vec<SeccionUI> = self.secciones.iter()
            .filter(|s| s.visible && s.lado == LadoLayout::Derecha)
            .cloned().collect();
        for s in &derecha {
            let w = resolver_tamano(&s.tamano_pref, resto.ancho);
            resto.ancho -= w;
            self.soluciones.push(GeometriaSolucion {
                id: s.id.clone(),
                rect: RectPx { x: resto.x + resto.ancho, y: resto.y, ancho: w, alto: resto.alto },
            });
        }

        // 5. Centro
        if let Some(s) = self.secciones.iter().find(|s| s.visible && s.lado == LadoLayout::Centro) {
            self.soluciones.push(GeometriaSolucion {
                id: s.id.clone(),
                rect: RectPx { x: resto.x, y: resto.y, ancho: resto.ancho, alto: resto.alto },
            });
        }

        &self.soluciones
    }

    /// Devuelve el ID de la sección en la que cae el punto (x, y), o None.
    /// Las secciones con z_order mayor se comprueban primero.
    pub fn seccion_en_posicion(&self, x: f32, y: f32) -> Option<&String> {
        self.soluciones.iter().rev().find(|sol| {
            let r = &sol.rect;
            x >= r.x && x < r.x + r.ancho && y >= r.y && y < r.y + r.alto
        }).map(|sol| &sol.id)
    }

    pub fn rect_seccion(&self, id: &str) -> Option<&RectPx> {
        self.soluciones.iter().find(|sol| sol.id == id).map(|sol| &sol.rect)
    }

    pub fn secciones(&self) -> &[SeccionUI] {
        &self.secciones
    }
}

fn resolver_tamano(pref: &TamanoPref, disponible: f32) -> f32 {
    match pref {
        TamanoPref::Fijo(v)         => v.min(disponible),
        TamanoPref::Flex(f)         => disponible * f,
        TamanoPref::Minmax(min, max) => disponible.clamp(*min, *max),
    }
}
