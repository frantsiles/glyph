// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # ContextoGpu
//!
//! Encapsula todos los objetos wgpu de larga vida: instancia, adaptador,
//! dispositivo, cola y configuración de la superficie de renderizado.

use anyhow::{anyhow, Result};
use std::sync::Arc;
use winit::window::Window;

/// Estado GPU completo asociado a una ventana
pub struct ContextoGpu {
    /// Superficie de renderizado vinculada a la ventana del OS
    pub superficie: wgpu::Surface<'static>,

    /// Dispositivo lógico — abstracción sobre la GPU real
    pub dispositivo: wgpu::Device,

    /// Cola de comandos para enviar work a la GPU
    pub cola: wgpu::Queue,

    /// Configuración activa de la superficie (formato, tamaño, vsync…)
    pub config_superficie: wgpu::SurfaceConfiguration,
}

impl ContextoGpu {
    /// Inicializa el contexto GPU completo de forma asíncrona.
    ///
    /// Se usa `pollster::block_on` en el event loop para bloquear solo durante
    /// el arranque — no afecta la latencia de frames una vez iniciado.
    pub async fn nuevo(ventana: Arc<Window>) -> Result<Self> {
        let instancia = wgpu::Instance::default();

        // Surface<'static> — seguro porque Arc<Window> mantiene la ventana viva
        let superficie = instancia
            .create_surface(ventana.clone())
            .map_err(|e| anyhow!("Error creando superficie wgpu: {e}"))?;

        let adaptador = instancia
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&superficie),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow!("No se encontró adaptador GPU compatible"))?;

        let (dispositivo, cola) = adaptador
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("glyph_dispositivo"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .map_err(|e| anyhow!("Error solicitando dispositivo GPU: {e}"))?;

        let capacidades = superficie.get_capabilities(&adaptador);

        // Preferir formato sRGB si está disponible
        let formato = capacidades
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(capacidades.formats[0]);

        let tamaño = ventana.inner_size();
        let config_superficie = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: formato,
            width: tamaño.width.max(1),
            height: tamaño.height.max(1),
            present_mode: wgpu::PresentMode::Fifo, // vsync
            alpha_mode: capacidades.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        superficie.configure(&dispositivo, &config_superficie);

        tracing::info!(
            "GPU inicializada: {:?} | formato {:?}",
            adaptador.get_info().name,
            formato
        );

        Ok(Self {
            superficie,
            dispositivo,
            cola,
            config_superficie,
        })
    }

    /// Reconfigura la superficie tras un cambio de tamaño de ventana
    pub fn redimensionar(&mut self, nuevo_ancho: u32, nuevo_alto: u32) {
        if nuevo_ancho == 0 || nuevo_alto == 0 {
            return;
        }
        self.config_superficie.width = nuevo_ancho;
        self.config_superficie.height = nuevo_alto;
        self.superficie
            .configure(&self.dispositivo, &self.config_superficie);
    }
}
