// Copyright 2026 Franz (frantsiles)
// Licensed under the Apache License, Version 2.0

//! # QuadRenderer
//!
//! Pipeline wgpu para renderizar rectángulos coloreados (fondos de secciones).
//!
//! Orden de renderizado: el `QuadRenderer` se ejecuta ANTES del `RendererTexto`
//! para que el texto quede sobre los fondos de sección.

const SHADER: &str = r#"
struct VertOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
}

struct Uniforms {
    screen_size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
) -> VertOut {
    let x = (pos.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let y = 1.0 - (pos.y / uniforms.screen_size.y) * 2.0;
    return VertOut(vec4<f32>(x, y, 0.0, 1.0), color);
}

@fragment
fn fs_main(in: VertOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

/// Rect en píxeles (origen top-left)
#[derive(Debug, Clone, Copy)]
pub struct RectPx {
    pub x: f32,
    pub y: f32,
    pub ancho: f32,
    pub alto: f32,
}

/// Color RGBA normalizado (0.0–1.0)
#[derive(Debug, Clone, Copy)]
pub struct ColorRgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl ColorRgba {
    pub fn rgb_u8(r: u8, g: u8, b: u8) -> Self {
        Self { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct QuadVertex {
    pos: [f32; 2],
    color: [f32; 4],
}

const VERTICES_POR_QUAD: usize = 6;
const QUADS_INICIAL: usize = 64;

pub struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertices: Vec<QuadVertex>,
    capacidad_vertices: usize,
}

impl QuadRenderer {
    pub fn nuevo(dispositivo: &wgpu::Device, formato: wgpu::TextureFormat) -> Self {
        let shader = dispositivo.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let uniform_buffer = dispositivo.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad_uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bgl = dispositivo.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quad_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = dispositivo.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quad_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = dispositivo.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad_pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 8, shader_location: 1 },
            ],
        };

        let pipeline = dispositivo.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[vbl] },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: formato,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let capacidad_vertices = QUADS_INICIAL * VERTICES_POR_QUAD;
        let vertex_buffer = dispositivo.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad_vb"),
            size: (capacidad_vertices * std::mem::size_of::<QuadVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self { pipeline, vertex_buffer, uniform_buffer, bind_group, vertices: Vec::new(), capacidad_vertices }
    }

    pub fn limpiar(&mut self) {
        self.vertices.clear();
    }

    pub fn agregar_quad(&mut self, rect: RectPx, color: ColorRgba) {
        let c = [color.r, color.g, color.b, color.a];
        let (x0, y0, x1, y1) = (rect.x, rect.y, rect.x + rect.ancho, rect.y + rect.alto);
        self.vertices.extend_from_slice(&[
            QuadVertex { pos: [x0, y0], color: c },
            QuadVertex { pos: [x1, y0], color: c },
            QuadVertex { pos: [x0, y1], color: c },
            QuadVertex { pos: [x1, y0], color: c },
            QuadVertex { pos: [x1, y1], color: c },
            QuadVertex { pos: [x0, y1], color: c },
        ]);
    }

    /// Sube los datos de vértices a la GPU. Llama antes del render pass.
    pub fn preparar(&mut self, dispositivo: &wgpu::Device, cola: &wgpu::Queue, ancho: u32, alto: u32) {
        let screen: [f32; 4] = [ancho as f32, alto as f32, 0.0, 0.0];
        let bytes = unsafe {
            std::slice::from_raw_parts(screen.as_ptr() as *const u8, 16)
        };
        cola.write_buffer(&self.uniform_buffer, 0, bytes);

        if self.vertices.is_empty() { return; }

        if self.vertices.len() > self.capacidad_vertices {
            self.capacidad_vertices = self.vertices.len() * 2;
            self.vertex_buffer = dispositivo.create_buffer(&wgpu::BufferDescriptor {
                label: Some("quad_vb"),
                size: (self.capacidad_vertices * std::mem::size_of::<QuadVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let bytes = unsafe {
            std::slice::from_raw_parts(
                self.vertices.as_ptr() as *const u8,
                self.vertices.len() * std::mem::size_of::<QuadVertex>(),
            )
        };
        cola.write_buffer(&self.vertex_buffer, 0, bytes);
    }

    pub fn renderizar_en_pase<'pass>(&'pass self, pase: &mut wgpu::RenderPass<'pass>) {
        if self.vertices.is_empty() { return; }
        pase.set_pipeline(&self.pipeline);
        pase.set_bind_group(0, &self.bind_group, &[]);
        pase.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pase.draw(0..self.vertices.len() as u32, 0..1);
    }
}
