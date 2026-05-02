// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! Plugin WASM — tema Monokai para Glyph.
//!
//! Compilar:
//!   cargo build --target wasm32-unknown-unknown --release
//!   # output: target/wasm32-unknown-unknown/release/plugin_wasm_monokai.wasm

#![no_std]

use core::ptr;
use core::slice;
use core::str;

// ------------------------------------------------------------------
// Allocator mínimo (bump) — suficiente para un plugin de solo-lectura
// ------------------------------------------------------------------

const HEAP_SIZE: usize = 32 * 1024;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
static mut HEAP_PTR: usize = 0;

/// Alloca `size` bytes alineados a 4 y retorna el puntero.
/// El host llama esto para escribir strings en nuestra memoria.
#[no_mangle]
pub unsafe extern "C" fn glyph_alloc(size: i32) -> i32 {
    let size = size as usize;
    // alinear a 4
    let aligned = (HEAP_PTR + 3) & !3;
    if aligned + size > HEAP_SIZE {
        return 0;
    }
    HEAP_PTR = aligned + size;
    (HEAP.as_ptr() as usize + aligned) as i32
}

// ------------------------------------------------------------------
// Helpers de construcción de respuestas
// ------------------------------------------------------------------

// Área estática para respuestas — cada hook escribe aquí su JSON.
static mut RESP_BUF: [u8; 8 * 1024] = [0u8; 8 * 1024];

/// Escribe `[u32-LE len][UTF-8 bytes]` en RESP_BUF y devuelve el puntero.
unsafe fn respuesta(s: &[u8]) -> i32 {
    let len = s.len() as u32;
    let len_bytes = len.to_le_bytes();
    RESP_BUF[0..4].copy_from_slice(&len_bytes);
    RESP_BUF[4..4 + s.len()].copy_from_slice(s);
    RESP_BUF.as_ptr() as i32
}

/// Respuesta vacía: JSON `[]` lista vacía de acciones.
unsafe fn respuesta_vacia() -> i32 {
    respuesta(b"[]")
}

// ------------------------------------------------------------------
// Metadatos
// ------------------------------------------------------------------

static METADATA: &[u8] = br#"{"nombre":"monokai","descripcion":"Tema Monokai clásico","permisos":{"ui":true}}"#;

#[no_mangle]
pub unsafe extern "C" fn glyph_metadata() -> i32 {
    respuesta(METADATA)
}

// ------------------------------------------------------------------
// Hooks
// ------------------------------------------------------------------

/// Inicializar: devuelve el tema Monokai como AccionPlugin::EstablecerTema.
#[no_mangle]
pub unsafe extern "C" fn glyph_inicializar() -> i32 {
    // Paleta Monokai clásica
    let json = br#"[{"EstablecerTema":{"keyword":"#F92672","string":"#E6DB74","comment":"#75715E","function":"#A6E22E","type":"#66D9EF","number":"#AE81FF","operator":"#F92672","variable":"#F8F8F2","constant":"#AE81FF","punctuation":"#F8F8F2","attribute":"#A6E22E","default":"#F8F8F2"}}]"#;
    respuesta(json)
}

/// al_abrir: sin comportamiento especial en este plugin.
#[no_mangle]
pub unsafe extern "C" fn glyph_al_abrir(_ruta_ptr: i32, _ruta_len: i32) -> i32 {
    respuesta_vacia()
}

/// al_cambiar: sin comportamiento especial en este plugin.
#[no_mangle]
pub unsafe extern "C" fn glyph_al_cambiar(_ruta_ptr: i32, _ruta_len: i32, _version: i32) -> i32 {
    respuesta_vacia()
}

/// al_guardar: sin comportamiento especial en este plugin.
#[no_mangle]
pub unsafe extern "C" fn glyph_al_guardar(_ruta_ptr: i32, _ruta_len: i32) -> i32 {
    respuesta_vacia()
}

// ------------------------------------------------------------------
// Panic handler (requerido en no_std)
// ------------------------------------------------------------------

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
