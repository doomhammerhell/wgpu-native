use hal::{self, Device as _Device};
use hal::queue::RawCommandQueue;
use {binding_model, command, conv, memory, pipeline, resource};

use std::{iter, slice};
use registry::{self, Items, Registry};
use {
    AttachmentStateId, BindGroupLayoutId, BlendStateId, BufferId, CommandBufferId, DepthStencilStateId, DeviceId,
    PipelineLayoutId, QueueId, RenderPipelineId, ShaderModuleId,
};


pub struct Device<B: hal::Backend> {
    device: B::Device,
    queue_group: hal::QueueGroup<B, hal::General>,
    mem_allocator: memory::SmartAllocator<B>,
    com_allocator: command::CommandAllocator<B>,
}

impl<B: hal::Backend> Device<B> {
    pub(crate) fn new(
        device: B::Device,
        queue_group: hal::QueueGroup<B, hal::General>,
        mem_props: hal::MemoryProperties,
    ) -> Self {
        Device {
            device,
            mem_allocator: memory::SmartAllocator::new(mem_props, 1, 1, 1, 1),
            com_allocator: command::CommandAllocator::new(queue_group.family()),
            queue_group,
        }
    }
}

pub(crate) struct ShaderModule<B: hal::Backend> {
    pub raw: B::ShaderModule,
}

#[no_mangle]
pub extern "C" fn wgpu_device_create_bind_group_layout(
    device_id: DeviceId,
    desc: binding_model::BindGroupLayoutDescriptor,
) -> BindGroupLayoutId {
    let bindings = unsafe { slice::from_raw_parts(desc.bindings, desc.bindings_length) };
    let device_guard = registry::DEVICE_REGISTRY.lock();
    let device = device_guard.get(device_id);
    let descriptor_set_layout = device.device.create_descriptor_set_layout(
        bindings.iter().map(|binding| {
            hal::pso::DescriptorSetLayoutBinding {
                binding: binding.binding,
                ty: conv::map_binding_type(&binding.ty),
                count: bindings.len(),
                stage_flags: conv::map_shader_stage_flags(binding.visibility),
                immutable_samplers: false, // TODO
            }
        }),
        &[],
    );
    registry::BIND_GROUP_LAYOUT_REGISTRY.register(binding_model::BindGroupLayout {
        raw: descriptor_set_layout,
    })
}

#[no_mangle]
pub extern "C" fn wgpu_device_create_pipeline_layout(
    device_id: DeviceId,
    desc: binding_model::PipelineLayoutDescriptor,
) -> PipelineLayoutId {
    let bind_group_layout_guard = registry::BIND_GROUP_LAYOUT_REGISTRY.lock();
    let descriptor_set_layouts =
        unsafe { slice::from_raw_parts(desc.bind_group_layouts, desc.bind_group_layouts_length) }
            .iter()
            .map(|id| bind_group_layout_guard.get(id.clone()))
            .collect::<Vec<_>>();
    let device_guard = registry::DEVICE_REGISTRY.lock();
    let device = &device_guard.get(device_id).device;
    let pipeline_layout =
        device.create_pipeline_layout(descriptor_set_layouts.iter().map(|d| &d.raw), &[]); // TODO: push constants
    registry::PIPELINE_LAYOUT_REGISTRY.register(binding_model::PipelineLayout {
        raw: pipeline_layout,
    })
}

#[no_mangle]
pub extern "C" fn wgpu_device_create_blend_state(
    _device_id: DeviceId,
    desc: pipeline::BlendStateDescriptor,
) -> BlendStateId {
    registry::BLEND_STATE_REGISTRY.register(pipeline::BlendState {
        raw: conv::map_blend_state_descriptor(desc),
    })
}

#[no_mangle]
pub extern "C" fn wgpu_device_create_depth_stencil_state(
    device_id: DeviceId,
    desc: pipeline::DepthStencilStateDescriptor,
) -> DepthStencilStateId {
    registry::DEPTH_STENCIL_STATE_REGISTRY.register(pipeline::DepthStencilState {
        raw: conv::map_depth_stencil_state(desc),
    })
}

#[no_mangle]
pub extern "C" fn wgpu_device_create_shader_module(
    device_id: DeviceId,
    desc: pipeline::ShaderModuleDescriptor,
) -> ShaderModuleId {
    let device_guard = registry::DEVICE_REGISTRY.lock();
    let device = &device_guard.get(device_id).device;
    let shader = device
        .create_shader_module(unsafe { slice::from_raw_parts(desc.code.bytes, desc.code.length) })
        .unwrap();
    registry::SHADER_MODULE_REGISTRY.register(ShaderModule { raw: shader })
}

#[no_mangle]
pub extern "C" fn wgpu_device_create_command_buffer(
    device_id: DeviceId,
    _desc: command::CommandBufferDescriptor,
) -> CommandBufferId {
    let device = registry::DEVICE_REGISTRY.get_mut(device_id);
    let cmd_buf = device.com_allocator.allocate(&device.device);
    registry::COMMAND_BUFFER_REGISTRY.register(cmd_buf)
}

#[no_mangle]
pub extern "C" fn wgpu_device_get_queue(
    device_id: DeviceId,
) -> QueueId {
   device_id
}

#[no_mangle]
pub extern "C" fn wgpu_queue_submit(
    queue_id: QueueId,
    command_buffer_ptr: *const CommandBufferId,
    command_buffer_count: usize,
) {
    let mut device = registry::DEVICE_REGISTRY.get_mut(queue_id);
    let command_buffer_ids = unsafe {
        slice::from_raw_parts(command_buffer_ptr, command_buffer_count)
    };
    //TODO: submit at once, requires `get_all()`
    for &cmb_id in command_buffer_ids {
        let cmd_buf = registry::COMMAND_BUFFER_REGISTRY.take(cmb_id);
        {
            let submission = hal::queue::RawSubmission {
                cmd_buffers: iter::once(&cmd_buf.raw),
                wait_semaphores: &[],
                signal_semaphores: &[],
            };
            unsafe {
                device.queue_group.queues[0]
                    .as_raw_mut()
                    .submit_raw(submission, None);
            }
        }
        device.com_allocator.submit(cmd_buf);
    }
pub extern "C" fn wgpu_device_create_attachment_state(
    device_id: DeviceId,
    desc: pipeline::AttachmentStateDescriptor,
) -> AttachmentStateId {
    // TODO: Assume that `AttachmentStateDescriptor` contains multiple attachments.
    let attachments = unsafe { slice::from_raw_parts(desc.formats, desc.formats_length) }
        .iter()
        .map(|format| {
            hal::pass::Attachment {
                format: Some(conv::map_texture_format(*format)),
                samples: 1, // TODO map
                ops: hal::pass::AttachmentOps {
                    // TODO map
                    load: hal::pass::AttachmentLoadOp::Clear,
                    store: hal::pass::AttachmentStoreOp::Store,
                },
                stencil_ops: hal::pass::AttachmentOps {
                    // TODO map
                    load: hal::pass::AttachmentLoadOp::DontCare,
                    store: hal::pass::AttachmentStoreOp::DontCare,
                },
                layouts: hal::image::Layout::Undefined..hal::image::Layout::Present, // TODO map
            }
        }).collect();
    registry::ATTACHMENT_STATE_REGISTRY.register(pipeline::AttachmentState { raw: attachments })
}

#[no_mangle]
pub extern "C" fn wgpu_device_create_render_pipeline(
    device_id: DeviceId,
    desc: pipeline::RenderPipelineDescriptor,
) -> RenderPipelineId {
    // TODO
    let extent = hal::window::Extent2D {
        width: 100,
        height: 100,
    };

    let device_guard = registry::DEVICE_REGISTRY.lock();
    let device = &device_guard.get(device_id).device;
    let pipeline_layout_guard = registry::PIPELINE_LAYOUT_REGISTRY.lock();
    let layout = &pipeline_layout_guard.get(desc.layout).raw;
    let pipeline_stages = unsafe { slice::from_raw_parts(desc.stages, desc.stages_length) };
    let shader_module_guard = registry::SHADER_MODULE_REGISTRY.lock();
    let shaders = {
        let mut vertex = None;
        let mut fragment = None;
        for pipeline_stage in pipeline_stages.iter() {
            let entry_name = unsafe { ffi::CStr::from_ptr(pipeline_stage.entry_point) }
                .to_str()
                .to_owned()
                .unwrap();
            let entry = hal::pso::EntryPoint::<back::Backend> {
                entry: unsafe { ffi::CStr::from_ptr(pipeline_stage.entry_point) }
                    .to_str()
                    .to_owned()
                    .unwrap(), // TODO
                module: &shader_module_guard.get(pipeline_stage.module).raw,
                specialization: hal::pso::Specialization {
                    // TODO
                    constants: &[],
                    data: &[],
                },
            };
            match pipeline_stage.stage {
                pipeline::ShaderStage::Vertex => {
                    vertex = Some(entry);
                }
                pipeline::ShaderStage::Fragment => {
                    fragment = Some(entry);
                }
                pipeline::ShaderStage::Compute => unimplemented!(), // TODO
            }
        }

        hal::pso::GraphicsShaderSet {
            vertex: vertex.unwrap(), // TODO
            hull: None,
            domain: None,
            geometry: None,
            fragment,
        }
    };

    // TODO
    let rasterizer = hal::pso::Rasterizer {
        depth_clamping: false,
        polygon_mode: hal::pso::PolygonMode::Fill,
        cull_face: hal::pso::Face::BACK,
        front_face: hal::pso::FrontFace::Clockwise,
        depth_bias: None,
        conservative: false,
    };

    // TODO
    let vertex_buffers: Vec<hal::pso::VertexBufferDesc> = Vec::new();

    // TODO
    let attributes: Vec<hal::pso::AttributeDesc> = Vec::new();

    let input_assembler = hal::pso::InputAssemblerDesc {
        primitive: conv::map_primitive_topology(desc.primitive_topology),
        primitive_restart: hal::pso::PrimitiveRestart::Disabled, // TODO
    };

    let blend_state_guard = registry::BLEND_STATE_REGISTRY.lock();
    let blend_state = unsafe { slice::from_raw_parts(desc.blend_state, desc.blend_state_length) }
        .iter()
        .map(|id| blend_state_guard.get(id.clone()).raw)
        .collect();

    let blender = hal::pso::BlendDesc {
        logic_op: None, // TODO
        targets: blend_state,
    };

    let depth_stencil_state_guard = registry::DEPTH_STENCIL_STATE_REGISTRY.lock();
    let depth_stencil = depth_stencil_state_guard.get(desc.depth_stencil_state).raw;

    // TODO
    let multisampling: Option<hal::pso::Multisampling> = None;

    // TODO
    let baked_states = hal::pso::BakedStates {
        viewport: Some(hal::pso::Viewport {
            rect: hal::pso::Rect {
                x: 0,
                y: 0,
                w: extent.width as i16,
                h: extent.height as i16,
            },
            depth: (0.0..1.0),
        }),
        scissor: Some(hal::pso::Rect {
            x: 0,
            y: 0,
            w: extent.width as i16,
            h: extent.height as i16,
        }),
        blend_color: None,
        depth_bounds: None,
    };

    let attachment_state_guard = registry::ATTACHMENT_STATE_REGISTRY.lock();
    let attachments = &attachment_state_guard.get(desc.attachment_state).raw;

    // TODO
    let subpass = hal::pass::SubpassDesc {
        colors: &[(0, hal::image::Layout::ColorAttachmentOptimal)],
        depth_stencil: None,
        inputs: &[],
        resolves: &[],
        preserves: &[],
    };

    // TODO
    let subpass_dependency = hal::pass::SubpassDependency {
        passes: hal::pass::SubpassRef::External..hal::pass::SubpassRef::Pass(0),
        stages: hal::pso::PipelineStage::COLOR_ATTACHMENT_OUTPUT
            ..hal::pso::PipelineStage::COLOR_ATTACHMENT_OUTPUT,
        accesses: hal::image::Access::empty()
            ..(hal::image::Access::COLOR_ATTACHMENT_READ
                | hal::image::Access::COLOR_ATTACHMENT_WRITE),
    };

    let main_pass = &device.create_render_pass(&attachments[..], &[subpass], &[subpass_dependency]);

    // TODO
    let subpass = hal::pass::Subpass {
        index: 0,
        main_pass,
    };

    // TODO
    let flags = hal::pso::PipelineCreationFlags::empty();

    // TODO
    let parent = hal::pso::BasePipeline::None;

    let pipeline_desc = hal::pso::GraphicsPipelineDesc {
        shaders,
        rasterizer,
        vertex_buffers,
        attributes,
        input_assembler,
        blender,
        depth_stencil,
        multisampling,
        baked_states,
        layout,
        subpass,
        flags,
        parent,
    };

    // TODO: cache
    let pipeline = device
        .create_graphics_pipeline(&pipeline_desc, None)
        .unwrap();

    registry::RENDER_PIPELINE_REGISTRY.register(pipeline::RenderPipeline { raw: pipeline })
}