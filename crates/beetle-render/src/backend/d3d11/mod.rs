pub mod com;
pub mod shaders;

#[cfg(target_os = "windows")]
use com::*;
use super::{BlendMode, GpuBackend, TextureId, Vertex2D};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;

#[allow(dead_code)]
struct D3d11Texture {
    texture: *mut c_void,
    srv: *mut c_void,
    width: u32,
    height: u32,
}

/// Windows Direct3D 11 hardware-accelerated 2D graphics backend.
///
/// Features zero-crate native OS bindings, low-latency FLIP swapchain,
/// dynamic vertex/index buffer streaming, and hardware alpha/additive blending.
pub struct D3d11Backend {
    _hwnd: *mut c_void,
    width: u32,
    height: u32,
    device: *mut c_void,
    context: *mut c_void,
    swap_chain: *mut c_void,
    render_target_view: *mut c_void,
    vertex_buffer: *mut c_void,
    index_buffer: *mut c_void,
    constant_buffer: *mut c_void,
    input_layout: *mut c_void,
    vertex_shader: *mut c_void,
    pixel_shader_sprite: *mut c_void,
    pixel_shader_color: *mut c_void,
    blend_state_alpha: *mut c_void,
    blend_state_additive: *mut c_void,
    sampler_linear: *mut c_void,
    textures: HashMap<TextureId, D3d11Texture>,
    next_texture_id: u32,
}

unsafe impl Send for D3d11Backend {}
unsafe impl Sync for D3d11Backend {}

impl D3d11Backend {
    /// Attempts to initialize the Direct3D 11 device, swapchain, and 2D pipeline for a Windows HWND.
    pub fn new(hwnd: *mut c_void, width: u32, height: u32) -> Result<Self, String> {
        let w = width.max(1);
        let h = height.max(1);

        let swap_desc = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: DXGI_MODE_DESC {
                Width: w,
                Height: h,
                RefreshRate: DXGI_RATIONAL {
                    Numerator: 0,
                    Denominator: 1,
                },
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ScanlineOrdering: 0,
                Scaling: 0,
            },
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            OutputWindow: hwnd,
            Windowed: 1,
            SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
            Flags: 0,
        };

        let mut device: *mut c_void = ptr::null_mut();
        let mut context: *mut c_void = ptr::null_mut();
        let mut swap_chain: *mut c_void = ptr::null_mut();
        let mut feature_level: u32 = 0;

        let hr = unsafe {
            D3D11CreateDeviceAndSwapChain(
                ptr::null_mut(),
                D3D_DRIVER_TYPE_HARDWARE,
                ptr::null_mut(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                [D3D_FEATURE_LEVEL_11_0].as_ptr(),
                1,
                7, // D3D11_SDK_VERSION
                &swap_desc,
                &mut swap_chain,
                &mut device,
                &mut feature_level,
                &mut context,
            )
        };

        if hr < 0 || device.is_null() || context.is_null() || swap_chain.is_null() {
            return Err(format!("D3D11CreateDeviceAndSwapChain failed: 0x{:08X}", hr as u32));
        }

        // 1. Create Render Target View for Backbuffer
        let mut render_target_view = ptr::null_mut();
        unsafe {
            let sc_vtbl = *(swap_chain as *mut *mut IDXGISwapChainVtbl);
            let mut backbuffer: *mut c_void = ptr::null_mut();
            let hr_bb = ((*sc_vtbl).GetBuffer)(swap_chain, 0, &IID_ID3D11TEXTURE2D, &mut backbuffer);
            if hr_bb >= 0 && !backbuffer.is_null() {
                let dev_vtbl = *(device as *mut *mut ID3D11DeviceVtbl);
                ((*dev_vtbl).CreateRenderTargetView)(
                    device,
                    backbuffer,
                    ptr::null(),
                    &mut render_target_view,
                );
                let bb_vtbl = *(backbuffer as *mut *mut IUnknownVtbl);
                ((*bb_vtbl).Release)(backbuffer);
            }
        }

        // 2. Compile Shaders
        let vs_bytes = shaders::compile_hlsl(shaders::HLSL_2D_SOURCE, "VS_Main", "vs_4_0")?;
        let ps_sprite_bytes = shaders::compile_hlsl(shaders::HLSL_2D_SOURCE, "PS_Sprite", "ps_4_0")?;
        let ps_color_bytes = shaders::compile_hlsl(shaders::HLSL_2D_SOURCE, "PS_Color", "ps_4_0")?;

        let mut vertex_shader = ptr::null_mut();
        let mut pixel_shader_sprite = ptr::null_mut();
        let mut pixel_shader_color = ptr::null_mut();
        let mut input_layout = ptr::null_mut();

        unsafe {
            let dev_vtbl = *(device as *mut *mut ID3D11DeviceVtbl);
            ((*dev_vtbl).CreateVertexShader)(
                device,
                vs_bytes.as_ptr() as *const c_void,
                vs_bytes.len(),
                ptr::null_mut(),
                &mut vertex_shader,
            );
            ((*dev_vtbl).CreatePixelShader)(
                device,
                ps_sprite_bytes.as_ptr() as *const c_void,
                ps_sprite_bytes.len(),
                ptr::null_mut(),
                &mut pixel_shader_sprite,
            );
            ((*dev_vtbl).CreatePixelShader)(
                device,
                ps_color_bytes.as_ptr() as *const c_void,
                ps_color_bytes.len(),
                ptr::null_mut(),
                &mut pixel_shader_color,
            );

            // Input Layout (pos: R32G32_FLOAT, uv: R32G32_FLOAT, color: R32G32B32A32_FLOAT)
            let sem_pos = b"POSITION\0";
            let sem_uv = b"TEXCOORD\0";
            let sem_col = b"COLOR\0";
            let elements = [
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: sem_pos.as_ptr() as *const i8,
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: 0,
                    InputSlotClass: 0,
                    InstanceDataStepRate: 0,
                },
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: sem_uv.as_ptr() as *const i8,
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: 8,
                    InputSlotClass: 0,
                    InstanceDataStepRate: 0,
                },
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: sem_col.as_ptr() as *const i8,
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: 16,
                    InputSlotClass: 0,
                    InstanceDataStepRate: 0,
                },
            ];

            ((*dev_vtbl).CreateInputLayout)(
                device,
                elements.as_ptr(),
                3,
                vs_bytes.as_ptr() as *const c_void,
                vs_bytes.len(),
                &mut input_layout,
            );
        }

        // 3. Create Dynamic Vertex & Index Buffers (capacity for 8192 vertices and 12288 indices)
        let mut vertex_buffer = ptr::null_mut();
        let mut index_buffer = ptr::null_mut();
        let mut constant_buffer = ptr::null_mut();

        unsafe {
            let dev_vtbl = *(device as *mut *mut ID3D11DeviceVtbl);

            let vb_desc = D3D11_BUFFER_DESC {
                ByteWidth: (8192 * std::mem::size_of::<Vertex2D>()) as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_VERTEX_BUFFER,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE,
                MiscFlags: 0,
                StructureByteStride: 0,
            };
            let hr_vb = ((*dev_vtbl).CreateBuffer)(device, &vb_desc, ptr::null(), &mut vertex_buffer);

            let ib_desc = D3D11_BUFFER_DESC {
                ByteWidth: (12288 * std::mem::size_of::<u16>()) as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_INDEX_BUFFER,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE,
                MiscFlags: 0,
                StructureByteStride: 0,
            };
            let hr_ib = ((*dev_vtbl).CreateBuffer)(device, &ib_desc, ptr::null(), &mut index_buffer);

            let cb_desc = D3D11_BUFFER_DESC {
                ByteWidth: 16, // float4 (screen_w, screen_h, 0, 0)
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE,
                MiscFlags: 0,
                StructureByteStride: 0,
            };
            let hr_cb = ((*dev_vtbl).CreateBuffer)(device, &cb_desc, ptr::null(), &mut constant_buffer);

            if hr_vb < 0 || hr_ib < 0 || hr_cb < 0
                || vertex_buffer.is_null()
                || index_buffer.is_null()
                || constant_buffer.is_null()
            {
                return Err("Failed to create D3D11 dynamic buffers".to_string());
            }
        }

        // 4. Create Blend States (Alpha & Additive)
        let mut blend_state_alpha = ptr::null_mut();
        let mut blend_state_additive = ptr::null_mut();

        unsafe {
            let dev_vtbl = *(device as *mut *mut ID3D11DeviceVtbl);

            let mut blend_desc: D3D11_BLEND_DESC = std::mem::zeroed();
            blend_desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: 1,
                SrcBlend: D3D11_BLEND_SRC_ALPHA,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL,
            };
            ((*dev_vtbl).CreateBlendState)(device, &blend_desc, &mut blend_state_alpha);

            let mut add_desc: D3D11_BLEND_DESC = std::mem::zeroed();
            add_desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: 1,
                SrcBlend: D3D11_BLEND_SRC_ALPHA,
                DestBlend: D3D11_BLEND_ONE,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_ONE,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL,
            };
            ((*dev_vtbl).CreateBlendState)(device, &add_desc, &mut blend_state_additive);
        }

        // 5. Create Sampler State (Bilinear)
        let mut sampler_linear = ptr::null_mut();
        unsafe {
            let dev_vtbl = *(device as *mut *mut ID3D11DeviceVtbl);
            let samp_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                MipLODBias: 0.0,
                MaxAnisotropy: 1,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                BorderColor: [0.0, 0.0, 0.0, 0.0],
                MinLOD: 0.0,
                MaxLOD: f32::MAX,
            };
            ((*dev_vtbl).CreateSamplerState)(device, &samp_desc, &mut sampler_linear);
        }

        Ok(Self {
            _hwnd: hwnd,
            width: w,
            height: h,
            device,
            context,
            swap_chain,
            render_target_view,
            vertex_buffer,
            index_buffer,
            constant_buffer,
            input_layout,
            vertex_shader,
            pixel_shader_sprite,
            pixel_shader_color,
            blend_state_alpha,
            blend_state_additive,
            sampler_linear,
            textures: HashMap::new(),
            next_texture_id: 1,
        })
    }
}

impl Drop for D3d11Backend {
    fn drop(&mut self) {
        unsafe {
            for (_, tex) in self.textures.drain() {
                if !tex.srv.is_null() {
                    let vtbl = *(tex.srv as *mut *mut IUnknownVtbl);
                    let _ = ((*vtbl).Release)(tex.srv);
                }
                if !tex.texture.is_null() {
                    let vtbl = *(tex.texture as *mut *mut IUnknownVtbl);
                    let _ = ((*vtbl).Release)(tex.texture);
                }
            }

            macro_rules! release_com {
                ($field:expr) => {
                    if !$field.is_null() {
                        let vtbl = *($field as *mut *mut IUnknownVtbl);
                        let _ = ((*vtbl).Release)($field);
                        $field = ptr::null_mut();
                    }
                };
            }

            release_com!(self.sampler_linear);
            release_com!(self.blend_state_additive);
            release_com!(self.blend_state_alpha);
            release_com!(self.constant_buffer);
            release_com!(self.index_buffer);
            release_com!(self.vertex_buffer);
            release_com!(self.input_layout);
            release_com!(self.pixel_shader_color);
            release_com!(self.pixel_shader_sprite);
            release_com!(self.vertex_shader);
            release_com!(self.render_target_view);
            release_com!(self.swap_chain);
            release_com!(self.context);
            release_com!(self.device);
        }
    }
}

impl GpuBackend for D3d11Backend {
    fn begin_frame(&mut self, width: u32, height: u32, clear_color: [f32; 4]) {
        if self.width != width || self.height != height {
            self.resize(width, height);
        }

        unsafe {
            let ctx_vtbl = *(self.context as *mut *mut ID3D11DeviceContextVtbl);

            // Update Constant Buffer with Screen Dimensions
            let mut mapped: D3D11_MAPPED_SUBRESOURCE = std::mem::zeroed();
            let hr = ((*ctx_vtbl).Map)(
                self.context,
                self.constant_buffer,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                &mut mapped,
            );
            if hr >= 0 && !mapped.pData.is_null() {
                let screen_size = [self.width as f32, self.height as f32, 0.0f32, 0.0f32];
                ptr::copy_nonoverlapping(
                    screen_size.as_ptr() as *const c_void,
                    mapped.pData,
                    16,
                );
                ((*ctx_vtbl).Unmap)(self.context, self.constant_buffer, 0);
            }

            // Set Render Target & Viewport
            ((*ctx_vtbl).OMSetRenderTargets)(
                self.context,
                1,
                &self.render_target_view,
                ptr::null_mut(),
            );
            ((*ctx_vtbl).ClearRenderTargetView)(
                self.context,
                self.render_target_view,
                clear_color.as_ptr(),
            );

            let vp = D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.width as f32,
                Height: self.height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };
            ((*ctx_vtbl).RSSetViewports)(self.context, 1, &vp);

            // Bind Pipeline State
            ((*ctx_vtbl).IASetPrimitiveTopology)(self.context, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ((*ctx_vtbl).IASetInputLayout)(self.context, self.input_layout);
            ((*ctx_vtbl).VSSetShader)(self.context, self.vertex_shader, ptr::null(), 0);
            ((*ctx_vtbl).VSSetConstantBuffers)(self.context, 0, 1, &self.constant_buffer);
            ((*ctx_vtbl).PSSetSamplers)(self.context, 0, 1, &self.sampler_linear);
        }
    }

    fn create_texture(&mut self, width: u32, height: u32, pixels: &[u8]) -> Option<TextureId> {
        let id = TextureId(self.next_texture_id);
        self.next_texture_id += 1;

        let w = width.max(1);
        let h = height.max(1);
        let expected_bytes = (w * h * 4) as usize;

        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };

        let init_data = D3D11_SUBRESOURCE_DATA {
            pSysMem: pixels.as_ptr() as *const c_void,
            SysMemPitch: w * 4,
            SysMemSlicePitch: 0,
        };

        let mut texture = ptr::null_mut();
        let mut srv = ptr::null_mut();

        unsafe {
            let dev_vtbl = *(self.device as *mut *mut ID3D11DeviceVtbl);
            let hr = ((*dev_vtbl).CreateTexture2D)(
                self.device,
                &desc,
                if pixels.len() >= expected_bytes { &init_data } else { ptr::null() },
                &mut texture,
            );
            if hr < 0 || texture.is_null() {
                return None;
            }

            let hr_srv = ((*dev_vtbl).CreateShaderResourceView)(
                self.device,
                texture,
                ptr::null(),
                &mut srv,
            );
            if hr_srv < 0 || srv.is_null() {
                let vtbl = *(texture as *mut *mut IUnknownVtbl);
                let _ = ((*vtbl).Release)(texture);
                return None;
            }
        }

        self.textures.insert(
            id,
            D3d11Texture {
                texture,
                srv,
                width: w,
                height: h,
            },
        );

        Some(id)
    }

    fn update_texture(&mut self, id: TextureId, width: u32, height: u32, pixels: &[u8]) {
        let expected_bytes = (width * height * 4) as usize;
        if pixels.len() < expected_bytes {
            return;
        }

        if let Some(tex) = self.textures.get(&id) {
            if tex.width != width || tex.height != height {
                return;
            }
            unsafe {
                let ctx_vtbl = *(self.context as *mut *mut ID3D11DeviceContextVtbl);
                ((*ctx_vtbl).UpdateSubresource)(
                    self.context,
                    tex.texture,
                    0,
                    ptr::null(),
                    pixels.as_ptr() as *const c_void,
                    width * 4,
                    0,
                );
            }
        }
    }

    fn destroy_texture(&mut self, id: TextureId) {
        if let Some(tex) = self.textures.remove(&id) {
            unsafe {
                if !tex.srv.is_null() {
                    let vtbl = *(tex.srv as *mut *mut IUnknownVtbl);
                    let _ = ((*vtbl).Release)(tex.srv);
                }
                if !tex.texture.is_null() {
                    let vtbl = *(tex.texture as *mut *mut IUnknownVtbl);
                    let _ = ((*vtbl).Release)(tex.texture);
                }
            }
        }
    }

    fn draw_batch(
        &mut self,
        vertices: &[Vertex2D],
        indices: &[u16],
        texture: Option<TextureId>,
        blend_mode: BlendMode,
    ) {
        if vertices.is_empty() || indices.is_empty() || vertices.len() > 8192 || indices.len() > 12288 {
            return;
        }

        unsafe {
            let ctx_vtbl = *(self.context as *mut *mut ID3D11DeviceContextVtbl);

            // 1. Upload Vertices
            let mut mapped_vb: D3D11_MAPPED_SUBRESOURCE = std::mem::zeroed();
            let hr_vb = ((*ctx_vtbl).Map)(
                self.context,
                self.vertex_buffer,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                &mut mapped_vb,
            );
            if hr_vb < 0 || mapped_vb.pData.is_null() {
                return;
            }
            let byte_count = vertices.len() * std::mem::size_of::<Vertex2D>();
            ptr::copy_nonoverlapping(
                vertices.as_ptr() as *const c_void,
                mapped_vb.pData,
                byte_count,
            );
            ((*ctx_vtbl).Unmap)(self.context, self.vertex_buffer, 0);

            // 2. Upload Indices
            let mut mapped_ib: D3D11_MAPPED_SUBRESOURCE = std::mem::zeroed();
            let hr_ib = ((*ctx_vtbl).Map)(
                self.context,
                self.index_buffer,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                &mut mapped_ib,
            );
            if hr_ib < 0 || mapped_ib.pData.is_null() {
                return;
            }
            let byte_count = indices.len() * std::mem::size_of::<u16>();
            ptr::copy_nonoverlapping(
                indices.as_ptr() as *const c_void,
                mapped_ib.pData,
                byte_count,
            );
            ((*ctx_vtbl).Unmap)(self.context, self.index_buffer, 0);

            // 3. Set Vertex & Index Buffers
            let stride = std::mem::size_of::<Vertex2D>() as u32;
            let offset = 0u32;
            ((*ctx_vtbl).IASetVertexBuffers)(
                self.context,
                0,
                1,
                &self.vertex_buffer,
                &stride,
                &offset,
            );
            ((*ctx_vtbl).IASetIndexBuffer)(
                self.context,
                self.index_buffer,
                DXGI_FORMAT_R16_UINT,
                0,
            );

            // 4. Set Blend Mode
            let blend_state = match blend_mode {
                BlendMode::Alpha => self.blend_state_alpha,
                BlendMode::Additive => self.blend_state_additive,
            };
            ((*ctx_vtbl).OMSetBlendState)(self.context, blend_state, ptr::null(), 0xffffffff);

            // 5. Set Texture & Pixel Shader
            if let Some(id) = texture {
                if let Some(tex) = self.textures.get(&id) {
                    ((*ctx_vtbl).PSSetShader)(self.context, self.pixel_shader_sprite, ptr::null(), 0);
                    ((*ctx_vtbl).PSSetShaderResources)(self.context, 0, 1, &tex.srv);
                } else {
                    ((*ctx_vtbl).PSSetShader)(self.context, self.pixel_shader_color, ptr::null(), 0);
                }
            } else {
                ((*ctx_vtbl).PSSetShader)(self.context, self.pixel_shader_color, ptr::null(), 0);
            }

            // 6. Draw Call
            ((*ctx_vtbl).DrawIndexed)(self.context, indices.len() as u32, 0, 0);
        }
    }

    fn end_frame(&mut self) {
        unsafe {
            let sc_vtbl = *(self.swap_chain as *mut *mut IDXGISwapChainVtbl);
            // Present 0: un-throttled presentation (game engine manages target FPS loop)
            let _ = ((*sc_vtbl).Present)(self.swap_chain, 0, 0);
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        self.width = w;
        self.height = h;

        unsafe {
            let ctx_vtbl = *(self.context as *mut *mut ID3D11DeviceContextVtbl);
            let sc_vtbl = *(self.swap_chain as *mut *mut IDXGISwapChainVtbl);

            // Release RTV before resizing swapchain
            if !self.render_target_view.is_null() {
                let vtbl = *(self.render_target_view as *mut *mut IUnknownVtbl);
                let _ = ((*vtbl).Release)(self.render_target_view);
                self.render_target_view = ptr::null_mut();
            }

            ((*ctx_vtbl).OMSetRenderTargets)(self.context, 0, ptr::null(), ptr::null_mut());

            let _ = ((*sc_vtbl).ResizeBuffers)(
                self.swap_chain,
                0,
                w,
                h,
                DXGI_FORMAT_R8G8B8A8_UNORM,
                0,
            );

            // Re-create RTV
            let mut backbuffer: *mut c_void = ptr::null_mut();
            let hr = ((*sc_vtbl).GetBuffer)(self.swap_chain, 0, &IID_ID3D11TEXTURE2D, &mut backbuffer);
            if hr >= 0 && !backbuffer.is_null() {
                let dev_vtbl = *(self.device as *mut *mut ID3D11DeviceVtbl);
                ((*dev_vtbl).CreateRenderTargetView)(
                    self.device,
                    backbuffer,
                    ptr::null(),
                    &mut self.render_target_view,
                );
                let bb_vtbl = *(backbuffer as *mut *mut IUnknownVtbl);
                let _ = ((*bb_vtbl).Release)(backbuffer);
            }
        }
    }

    fn backend_name(&self) -> &'static str {
        "Direct3D 11 (Hardware Accelerated)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_d3d11_shader_compilation() {
        let vs_res = shaders::compile_hlsl(shaders::HLSL_2D_SOURCE, "VS_Main", "vs_4_0");
        assert!(vs_res.is_ok(), "VS compilation should succeed: {:?}", vs_res.err());
        let vs_bytes = vs_res.unwrap();
        assert!(!vs_bytes.is_empty());

        let ps_sprite_res = shaders::compile_hlsl(shaders::HLSL_2D_SOURCE, "PS_Sprite", "ps_4_0");
        assert!(ps_sprite_res.is_ok(), "PS_Sprite compilation should succeed: {:?}", ps_sprite_res.err());

        let ps_color_res = shaders::compile_hlsl(shaders::HLSL_2D_SOURCE, "PS_Color", "ps_4_0");
        assert!(ps_color_res.is_ok(), "PS_Color compilation should succeed: {:?}", ps_color_res.err());
    }
}
