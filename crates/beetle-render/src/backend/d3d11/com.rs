//! Direct3D 11 COM vtables and Win32 FFI definitions.
//! Zero external crate dependencies; links directly to system `d3d11.dll` and `dxgi.dll`.

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

pub const IID_ID3D11TEXTURE2D: GUID = GUID {
    data1: 0x6f15aaf2,
    data2: 0xd208,
    data3: 0x4e89,
    data4: [0x9a, 0xb4, 0x48, 0x95, 0x35, 0xd3, 0x4f, 0x9c],
};

// D3D11 enums and constants
pub const D3D_DRIVER_TYPE_HARDWARE: u32 = 1;
pub const D3D_FEATURE_LEVEL_11_0: u32 = 0xb000;
pub const D3D11_CREATE_DEVICE_BGRA_SUPPORT: u32 = 0x20;

pub const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
pub const DXGI_FORMAT_B8G8R8A8_UNORM: u32 = 87;
pub const DXGI_FORMAT_R16_UINT: u32 = 57;
pub const DXGI_FORMAT_R32G32_FLOAT: u32 = 16;
pub const DXGI_FORMAT_R32G32B32A32_FLOAT: u32 = 2;

pub const DXGI_USAGE_RENDER_TARGET_OUTPUT: u32 = 0x20;
pub const DXGI_SWAP_EFFECT_DISCARD: u32 = 0;
pub const DXGI_SWAP_EFFECT_FLIP_DISCARD: u32 = 4;

pub const D3D11_USAGE_DEFAULT: u32 = 0;
pub const D3D11_USAGE_IMMUTABLE: u32 = 1;
pub const D3D11_USAGE_DYNAMIC: u32 = 2;

pub const D3D11_BIND_VERTEX_BUFFER: u32 = 0x1;
pub const D3D11_BIND_INDEX_BUFFER: u32 = 0x2;
pub const D3D11_BIND_CONSTANT_BUFFER: u32 = 0x4;
pub const D3D11_BIND_SHADER_RESOURCE: u32 = 0x8;
pub const D3D11_BIND_RENDER_TARGET: u32 = 0x20;

pub const D3D11_CPU_ACCESS_WRITE: u32 = 0x10000;
pub const D3D11_MAP_WRITE_DISCARD: u32 = 4;

pub const D3D11_FILTER_MIN_MAG_MIP_LINEAR: u32 = 0x15;
pub const D3D11_FILTER_MIN_MAG_MIP_POINT: u32 = 0;
pub const D3D11_TEXTURE_ADDRESS_CLAMP: u32 = 3;
pub const D3D11_COMPARISON_NEVER: u32 = 1;

pub const D3D11_BLEND_ZERO: u32 = 1;
pub const D3D11_BLEND_ONE: u32 = 2;
pub const D3D11_BLEND_SRC_ALPHA: u32 = 5;
pub const D3D11_BLEND_INV_SRC_ALPHA: u32 = 6;
pub const D3D11_BLEND_OP_ADD: u32 = 1;
pub const D3D11_COLOR_WRITE_ENABLE_ALL: u8 = 0xf;

pub const D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST: u32 = 4;

#[repr(C)]
pub struct DXGI_RATIONAL {
    pub Numerator: u32,
    pub Denominator: u32,
}

#[repr(C)]
pub struct DXGI_MODE_DESC {
    pub Width: u32,
    pub Height: u32,
    pub RefreshRate: DXGI_RATIONAL,
    pub Format: u32,
    pub ScanlineOrdering: u32,
    pub Scaling: u32,
}

#[repr(C)]
pub struct DXGI_SAMPLE_DESC {
    pub Count: u32,
    pub Quality: u32,
}

#[repr(C)]
pub struct DXGI_SWAP_CHAIN_DESC {
    pub BufferDesc: DXGI_MODE_DESC,
    pub SampleDesc: DXGI_SAMPLE_DESC,
    pub BufferUsage: u32,
    pub BufferCount: u32,
    pub OutputWindow: *mut c_void,
    pub Windowed: i32,
    pub SwapEffect: u32,
    pub Flags: u32,
}

#[repr(C)]
pub struct D3D11_BUFFER_DESC {
    pub ByteWidth: u32,
    pub Usage: u32,
    pub BindFlags: u32,
    pub CPUAccessFlags: u32,
    pub MiscFlags: u32,
    pub StructureByteStride: u32,
}

#[repr(C)]
pub struct D3D11_SUBRESOURCE_DATA {
    pub pSysMem: *const c_void,
    pub SysMemPitch: u32,
    pub SysMemSlicePitch: u32,
}

#[repr(C)]
pub struct D3D11_TEXTURE2D_DESC {
    pub Width: u32,
    pub Height: u32,
    pub MipLevels: u32,
    pub ArraySize: u32,
    pub Format: u32,
    pub SampleDesc: DXGI_SAMPLE_DESC,
    pub Usage: u32,
    pub BindFlags: u32,
    pub CPUAccessFlags: u32,
    pub MiscFlags: u32,
}

#[repr(C)]
pub struct D3D11_RENDER_TARGET_BLEND_DESC {
    pub BlendEnable: i32,
    pub SrcBlend: u32,
    pub DestBlend: u32,
    pub BlendOp: u32,
    pub SrcBlendAlpha: u32,
    pub DestBlendAlpha: u32,
    pub BlendOpAlpha: u32,
    pub RenderTargetWriteMask: u8,
}

#[repr(C)]
pub struct D3D11_BLEND_DESC {
    pub AlphaToCoverageEnable: i32,
    pub IndependentBlendEnable: i32,
    pub RenderTarget: [D3D11_RENDER_TARGET_BLEND_DESC; 8],
}

#[repr(C)]
pub struct D3D11_SAMPLER_DESC {
    pub Filter: u32,
    pub AddressU: u32,
    pub AddressV: u32,
    pub AddressW: u32,
    pub MipLODBias: f32,
    pub MaxAnisotropy: u32,
    pub ComparisonFunc: u32,
    pub BorderColor: [f32; 4],
    pub MinLOD: f32,
    pub MaxLOD: f32,
}

#[repr(C)]
pub struct D3D11_INPUT_ELEMENT_DESC {
    pub SemanticName: *const i8,
    pub SemanticIndex: u32,
    pub Format: u32,
    pub InputSlot: u32,
    pub AlignedByteOffset: u32,
    pub InputSlotClass: u32,
    pub InstanceDataStepRate: u32,
}

#[repr(C)]
pub struct D3D11_VIEWPORT {
    pub TopLeftX: f32,
    pub TopLeftY: f32,
    pub Width: f32,
    pub Height: f32,
    pub MinDepth: f32,
    pub MaxDepth: f32,
}

#[repr(C)]
pub struct D3D11_MAPPED_SUBRESOURCE {
    pub pData: *mut c_void,
    pub RowPitch: u32,
    pub DepthPitch: u32,
}

// COM VTABLES

#[repr(C)]
pub struct IUnknownVtbl {
    pub QueryInterface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32,
    pub AddRef: unsafe extern "system" fn(*mut c_void) -> u32,
    pub Release: unsafe extern "system" fn(*mut c_void) -> u32,
}

#[repr(C)]
pub struct IDXGISwapChainVtbl {
    pub parent: IUnknownVtbl, // 0..2: QueryInterface, AddRef, Release
    pub _unused1: [*const c_void; 5], // 3..7: SetPrivateData, SetPrivateDataInterface, GetPrivateData, GetParent, GetDevice
    pub Present: unsafe extern "system" fn(*mut c_void, u32, u32) -> i32, // 8
    pub GetBuffer: unsafe extern "system" fn(*mut c_void, u32, *const GUID, *mut *mut c_void) -> i32, // 9
    pub _unused2: [*const c_void; 3], // 10..12: SetFullscreenState, GetFullscreenState, GetDesc
    pub ResizeBuffers: unsafe extern "system" fn(*mut c_void, u32, u32, u32, u32, u32) -> i32, // 13
}

#[repr(C)]
pub struct ID3D11DeviceVtbl {
    pub parent: IUnknownVtbl, // 0..2: QueryInterface, AddRef, Release
    pub CreateBuffer: unsafe extern "system" fn(
        *mut c_void,
        *const D3D11_BUFFER_DESC,
        *const D3D11_SUBRESOURCE_DATA,
        *mut *mut c_void,
    ) -> i32, // 3
    pub _unused1: [*const c_void; 1], // 4: CreateTexture1D
    pub CreateTexture2D: unsafe extern "system" fn(
        *mut c_void,
        *const D3D11_TEXTURE2D_DESC,
        *const D3D11_SUBRESOURCE_DATA,
        *mut *mut c_void,
    ) -> i32, // 5
    pub _unused2: [*const c_void; 1], // 6: CreateTexture3D
    pub CreateShaderResourceView: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *const c_void,
        *mut *mut c_void,
    ) -> i32, // 7
    pub _unused3: [*const c_void; 1], // 8: CreateUnorderedAccessView
    pub CreateRenderTargetView: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *const c_void,
        *mut *mut c_void,
    ) -> i32, // 9
    pub _unused4: [*const c_void; 1], // 10: CreateDepthStencilView
    pub CreateInputLayout: unsafe extern "system" fn(
        *mut c_void,
        *const D3D11_INPUT_ELEMENT_DESC,
        u32,
        *const c_void,
        usize,
        *mut *mut c_void,
    ) -> i32, // 11
    pub CreateVertexShader: unsafe extern "system" fn(
        *mut c_void,
        *const c_void,
        usize,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32, // 12
    pub _unused5: [*const c_void; 2], // 13: CreateGeometryShader, 14: CreateGeometryShaderWithStreamOutput
    pub CreatePixelShader: unsafe extern "system" fn(
        *mut c_void,
        *const c_void,
        usize,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32, // 15
    pub _unused6: [*const c_void; 4], // 16: CreateHullShader, 17: CreateDomainShader, 18: CreateComputeShader, 19: CreateClassLinkage
    pub CreateBlendState: unsafe extern "system" fn(
        *mut c_void,
        *const D3D11_BLEND_DESC,
        *mut *mut c_void,
    ) -> i32, // 20
    pub _unused7: [*const c_void; 2], // 21: CreateDepthStencilState, 22: CreateRasterizerState
    pub CreateSamplerState: unsafe extern "system" fn(
        *mut c_void,
        *const D3D11_SAMPLER_DESC,
        *mut *mut c_void,
    ) -> i32, // 23
}

#[repr(C)]
pub struct ID3D11DeviceContextVtbl {
    pub parent: IUnknownVtbl, // 0..2: QueryInterface, AddRef, Release
    pub _unused_child: [*const c_void; 4], // 3..6: GetDevice, GetPrivateData, SetPrivateData, SetPrivateDataInterface
    pub VSSetConstantBuffers: unsafe extern "system" fn(*mut c_void, u32, u32, *const *mut c_void), // 7
    pub PSSetShaderResources: unsafe extern "system" fn(*mut c_void, u32, u32, *const *mut c_void), // 8
    pub PSSetShader: unsafe extern "system" fn(*mut c_void, *mut c_void, *const *mut c_void, u32), // 9
    pub PSSetSamplers: unsafe extern "system" fn(*mut c_void, u32, u32, *const *mut c_void), // 10
    pub VSSetShader: unsafe extern "system" fn(*mut c_void, *mut c_void, *const *mut c_void, u32), // 11
    pub DrawIndexed: unsafe extern "system" fn(*mut c_void, u32, u32, i32), // 12
    pub _unused_draw: [*const c_void; 1], // 13: Draw
    pub Map: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        u32,
        u32,
        *mut D3D11_MAPPED_SUBRESOURCE,
    ) -> i32, // 14
    pub Unmap: unsafe extern "system" fn(*mut c_void, *mut c_void, u32), // 15
    pub PSSetConstantBuffers: unsafe extern "system" fn(*mut c_void, u32, u32, *const *mut c_void), // 16
    pub IASetInputLayout: unsafe extern "system" fn(*mut c_void, *mut c_void), // 17
    pub IASetVertexBuffers: unsafe extern "system" fn(
        *mut c_void,
        u32,
        u32,
        *const *mut c_void,
        *const u32,
        *const u32,
    ), // 18
    pub IASetIndexBuffer: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, u32), // 19
    pub _unused_drawinst: [*const c_void; 4], // 20..23: DrawIndexedInstanced, DrawInstanced, GSSetConstantBuffers, GSSetShader
    pub IASetPrimitiveTopology: unsafe extern "system" fn(*mut c_void, u32), // 24
    pub _unused_queries: [*const c_void; 8], // 25..32: VSSetShaderResources, VSSetSamplers, Begin, End, GetData, SetPredication, GSSetShaderResources, GSSetSamplers
    pub OMSetRenderTargets: unsafe extern "system" fn(
        *mut c_void,
        u32,
        *const *mut c_void,
        *mut c_void,
    ), // 33
    pub _unused_uav: [*const c_void; 1], // 34: OMSetRenderTargetsAndUnorderedAccessViews
    pub OMSetBlendState: unsafe extern "system" fn(*mut c_void, *mut c_void, *const f32, u32), // 35
    pub _unused_rs: [*const c_void; 8], // 36..43: OMSetDepthStencilState, SOSetTargets, DrawAuto, DrawIndexedInstancedIndirect, DrawInstancedIndirect, Dispatch, DispatchIndirect, RSSetState
    pub RSSetViewports: unsafe extern "system" fn(*mut c_void, u32, *const D3D11_VIEWPORT), // 44
    pub _unused_copy: [*const c_void; 3], // 45..47: RSSetScissorRects, CopySubresourceRegion, CopyResource
    pub UpdateSubresource: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        *const c_void,
        *const c_void,
        u32,
        u32,
    ), // 48
    pub _unused_struct: [*const c_void; 1], // 49: CopyStructureCount
    pub ClearRenderTargetView: unsafe extern "system" fn(*mut c_void, *mut c_void, *const f32), // 50
}

#[repr(C)]
pub struct ID3D10BlobVtbl {
    pub parent: IUnknownVtbl,
    pub GetBufferPointer: unsafe extern "system" fn(*mut c_void) -> *mut c_void,
    pub GetBufferSize: unsafe extern "system" fn(*mut c_void) -> usize,
}

#[link(name = "d3d11")]
extern "system" {
    pub fn D3D11CreateDeviceAndSwapChain(
        pAdapter: *mut c_void,
        DriverType: u32,
        Software: *mut c_void,
        Flags: u32,
        pFeatureLevels: *const u32,
        FeatureLevels: u32,
        SDKVersion: u32,
        pSwapChainDesc: *const DXGI_SWAP_CHAIN_DESC,
        ppSwapChain: *mut *mut c_void,
        ppDevice: *mut *mut c_void,
        pFeatureLevel: *mut u32,
        ppImmediateContext: *mut *mut c_void,
    ) -> i32;
}

pub type D3DCompileFn = unsafe extern "system" fn(
    pSrcData: *const c_void,
    SrcDataSize: usize,
    pSourceName: *const i8,
    pDefines: *const c_void,
    pInclude: *mut c_void,
    pEntrypoint: *const i8,
    pTarget: *const i8,
    Flags1: u32,
    Flags2: u32,
    ppCode: *mut *mut c_void,
    ppErrorMsgs: *mut *mut c_void,
) -> i32;
