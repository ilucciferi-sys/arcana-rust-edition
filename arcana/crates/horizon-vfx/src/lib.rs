#![forbid(unsafe_code)]
use glam::{Mat4, Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Performance,
    High,
    Cinematic,
}
impl Quality {
    pub fn particles(self) -> u32 {
        match self {
            Self::Performance => 4096,
            Self::High => 16384,
            Self::Cinematic => 49152,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Accessibility {
    pub flashes: f32,
    pub camera_shake: f32,
    pub fov_pulse: f32,
}
impl Default for Accessibility {
    fn default() -> Self {
        Self {
            flashes: 0.6,
            camera_shake: 0.35,
            fov_pulse: 0.6,
        }
    }
}
/// Reusable energy-lance inputs. The caller supplies authoritative endpoints and
/// age; this crate knows no spell IDs, damage, networking or world simulation.
#[derive(Debug, Clone, Copy)]
pub struct Lance {
    pub start: Vec3,
    pub end: Vec3,
    pub age_seconds: f32,
    pub preparation_seconds: f32,
    pub impacted: bool,
    pub quality: Quality,
    pub accessibility: Accessibility,
}
impl Lance {
    pub fn visible(self) -> bool {
        self.age_seconds >= 0.0 && self.age_seconds < self.preparation_seconds + 3.5
    }
    pub fn camera_impulse(self) -> (Vec3, f32) {
        let t = self.age_seconds - self.preparation_seconds;
        let pulse = if t >= 0.0 { (-t * 10.0).exp() } else { 0.0 };
        (
            Vec3::new((t * 117.0).sin(), (t * 93.0).sin(), 0.0)
                * pulse
                * self.accessibility.camera_shake.clamp(0.0, 1.0)
                * 0.12,
            pulse * self.accessibility.fov_pulse.clamp(0.0, 1.0) * 4.0,
        )
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    vp: [[f32; 4]; 4],
    inverse: [[f32; 4]; 4],
    eye_time: [f32; 4],
    start_charge: [f32; 4],
    end_hit: [f32; 4],
    options: [f32; 4],
}
pub struct GpuVfx {
    uniform: wgpu::Buffer,
    particles: wgpu::Buffer,
    compute_group: wgpu::BindGroup,
    compute: wgpu::ComputePipeline,
    draw_layout: wgpu::BindGroupLayout,
    draw_group: Option<wgpu::BindGroup>,
    lance: wgpu::RenderPipeline,
    sprites: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    composite_group: Option<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    hdr: Option<wgpu::TextureView>,
    size: [u32; 2],
}
pub struct RenderTarget<'a> {
    pub color: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub size: [u32; 2],
    pub view_projection: Mat4,
    pub eye: Vec3,
}
fn buffer_entry(
    binding: u32,
    ty: wgpu::BufferBindingType,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
fn module(device: &wgpu::Device, name: &str, source: &'static str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(name),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}
fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    entries: (&str, &str),
    format: wgpu::TextureFormat,
    additive: bool,
    depth: bool,
) -> wgpu::RenderPipeline {
    let (vertex, fragment) = entries;
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(fragment),
        bind_group_layouts: &[Some(layout)],
        ..Default::default()
    });
    let blend = additive.then_some(wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent::OVER,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        multisample: Default::default(),
        depth_stencil: depth.then_some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
impl GpuVfx {
    pub fn new(device: &wgpu::Device, output: wgpu::TextureFormat) -> Self {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("VFX frame"),
            size: size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // A persistent pool: compute fills it directly; no per-particle CPU upload.
        let particles = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("VFX GPU pool"),
            size: 49152 * 48,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("VFX compute"),
            entries: &[
                buffer_entry(
                    0,
                    wgpu::BufferBindingType::Uniform,
                    wgpu::ShaderStages::COMPUTE,
                ),
                buffer_entry(
                    1,
                    wgpu::BufferBindingType::Storage { read_only: false },
                    wgpu::ShaderStages::COMPUTE,
                ),
            ],
        });
        let compute_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("VFX compute"),
            layout: &compute_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: particles.as_entire_binding(),
                },
            ],
        });
        let compute_shader = module(device, "VFX particles", include_str!("particles.wgsl"));
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("VFX compute"),
                bind_group_layouts: &[Some(&compute_layout)],
                ..Default::default()
            });
        let compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("VFX simulate"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: Some("simulate"),
            compilation_options: Default::default(),
            cache: None,
        });
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("VFX world"),
            entries: &[
                buffer_entry(
                    0,
                    wgpu::BufferBindingType::Uniform,
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                ),
                buffer_entry(
                    1,
                    wgpu::BufferBindingType::Storage { read_only: true },
                    wgpu::ShaderStages::VERTEX,
                ),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let draw = module(device, "VFX world", include_str!("draw.wgsl"));
        let lance = pipeline(
            device,
            &draw,
            &draw_layout,
            ("fullscreen", "lance"),
            wgpu::TextureFormat::Rgba16Float,
            true,
            false,
        );
        let sprites = pipeline(
            device,
            &draw,
            &draw_layout,
            ("sprite_vertex", "sprite_fragment"),
            wgpu::TextureFormat::Rgba16Float,
            true,
            true,
        );
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("VFX composite"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                buffer_entry(
                    2,
                    wgpu::BufferBindingType::Uniform,
                    wgpu::ShaderStages::FRAGMENT,
                ),
            ],
        });
        let post = module(device, "VFX bloom", include_str!("composite.wgsl"));
        let composite = pipeline(
            device,
            &post,
            &composite_layout,
            ("fullscreen", "composite"),
            output,
            true,
            false,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("VFX linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            uniform,
            particles,
            compute_group,
            compute,
            draw_layout,
            draw_group: None,
            lance,
            sprites,
            composite,
            composite_layout,
            composite_group: None,
            sampler,
            hdr: None,
            size: [0, 0],
        }
    }
    /// Reverse-Z, camera-relative or absolute coordinates; the matrices and
    /// endpoints must use the same space. Draw before the hand and HUD.
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: RenderTarget<'_>,
        effect: Lance,
    ) {
        let RenderTarget {
            color,
            depth,
            size,
            view_projection: vp,
            eye,
        } = target;
        if !effect.visible() || size.contains(&0) {
            return;
        }
        if self.size != size {
            self.size = size;
            self.hdr = Some(
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("VFX HDR emission"),
                        size: wgpu::Extent3d {
                            width: size[0],
                            height: size[1],
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba16Float,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default()),
            );
            self.composite_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("VFX bloom"),
                layout: &self.composite_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(self.hdr.as_ref().unwrap()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.uniform.as_entire_binding(),
                    },
                ],
            }));
        }
        // Depth attachments can be replaced without a resize (TAA/pack switch).
        self.draw_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("VFX world"),
            layout: &self.draw_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.particles.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        }));
        let count = effect.quality.particles();
        let data = Uniform {
            vp: vp.to_cols_array_2d(),
            inverse: vp.inverse().to_cols_array_2d(),
            eye_time: eye.extend(effect.age_seconds).to_array(),
            start_charge: effect.start.extend(effect.preparation_seconds).to_array(),
            end_hit: effect.end.extend(f32::from(effect.impacted)).to_array(),
            options: [
                count as f32,
                effect.accessibility.flashes.clamp(0.0, 1.0),
                f32::from(effect.quality != Quality::Performance),
                size[1] as f32,
            ],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&data));
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("VFX simulate"),
                ..Default::default()
            });
            pass.set_pipeline(&self.compute);
            pass.set_bind_group(0, &self.compute_group, &[]);
            pass.dispatch_workgroups(count.div_ceil(128), 1, 1);
        }
        let hdr = self.hdr.as_ref().unwrap();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("VFX lance"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: hdr,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.lance);
            pass.set_bind_group(0, self.draw_group.as_ref().unwrap(), &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("VFX GPU sprites"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: hdr,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.sprites);
            pass.set_bind_group(0, self.draw_group.as_ref().unwrap(), &[]);
            pass.draw(0..6, 0..count);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("VFX bloom composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.composite);
            pass.set_bind_group(0, self.composite_group.as_ref().unwrap(), &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shaders_validate() {
        for source in [
            include_str!("particles.wgsl"),
            include_str!("draw.wgsl"),
            include_str!("composite.wgsl"),
        ] {
            let m = naga::front::wgsl::parse_str(source).unwrap();
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&m)
            .unwrap();
        }
    }
    #[test]
    fn reduced_camera_effects_are_zero() {
        let e = super::Lance {
            start: glam::Vec3::ZERO,
            end: glam::Vec3::X,
            age_seconds: 3.55,
            preparation_seconds: 3.5,
            impacted: true,
            quality: super::Quality::High,
            accessibility: super::Accessibility {
                flashes: 0.0,
                camera_shake: 0.0,
                fov_pulse: 0.0,
            },
        };
        assert_eq!(e.camera_impulse(), (glam::Vec3::ZERO, 0.0));
    }
}
