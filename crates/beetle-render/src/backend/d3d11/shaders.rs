//! Direct3D 11 HLSL 2D Sprite & Primitive shaders with runtime dynamic compiler loader.

use super::com::{D3DCompileFn, ID3D10BlobVtbl};
use std::ffi::c_void;
use std::ptr;

pub const HLSL_2D_SOURCE: &str = r#"
cbuffer ConstantBuffer : register(b0) {
    float2 u_screen_size;
    float2 u_padding;
};

struct VS_INPUT {
    float2 pos : POSITION;
    float2 uv : TEXCOORD0;
    float4 color : COLOR0;
};

struct PS_INPUT {
    float4 pos : SV_POSITION;
    float2 uv : TEXCOORD0;
    float4 color : COLOR0;
};

PS_INPUT VS_Main(VS_INPUT input) {
    PS_INPUT output;
    output.pos = float4(input.pos.x / u_screen_size.x * 2.0 - 1.0, 1.0 - input.pos.y / u_screen_size.y * 2.0, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    return output;
}

Texture2D g_texture : register(t0);
SamplerState g_sampler : register(s0);

float4 PS_Sprite(PS_INPUT input) : SV_TARGET {
    return g_texture.Sample(g_sampler, input.uv) * input.color;
}

float4 PS_Color(PS_INPUT input) : SV_TARGET {
    return input.color;
}
"#;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(lpLibFileName: *const u16) -> *mut c_void;
    fn GetProcAddress(hModule: *mut c_void, lpProcName: *const i8) -> *mut c_void;
    fn FreeLibrary(hLibModule: *mut c_void) -> i32;
}

/// Dynamically compiles an HLSL source string using system `d3dcompiler_47.dll`.
pub fn compile_hlsl(source: &str, entry: &str, target: &str) -> Result<Vec<u8>, String> {
    let dll_name: Vec<u16> = "d3dcompiler_47.dll\0".encode_utf16().collect();
    let module = unsafe { LoadLibraryW(dll_name.as_ptr()) };
    if module.is_null() {
        return Err("Failed to load d3dcompiler_47.dll from system".to_string());
    }

    let proc_name = b"D3DCompile\0";
    let proc = unsafe { GetProcAddress(module, proc_name.as_ptr() as *const i8) };
    if proc.is_null() {
        unsafe { FreeLibrary(module) };
        return Err("D3DCompile function not found in d3dcompiler_47.dll".to_string());
    }

    let d3d_compile: D3DCompileFn = unsafe { std::mem::transmute(proc) };

    let entry_c = std::ffi::CString::new(entry).map_err(|e| e.to_string())?;
    let target_c = std::ffi::CString::new(target).map_err(|e| e.to_string())?;

    let mut code_blob: *mut c_void = ptr::null_mut();
    let mut error_blob: *mut c_void = ptr::null_mut();

    let hr = unsafe {
        d3d_compile(
            source.as_ptr() as *const c_void,
            source.len(),
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            entry_c.as_ptr(),
            target_c.as_ptr(),
            0,
            0,
            &mut code_blob,
            &mut error_blob,
        )
    };

    if hr < 0 {
        let err_msg = if !error_blob.is_null() {
            unsafe {
                let vtbl = *(error_blob as *mut *mut ID3D10BlobVtbl);
                let buf_ptr = ((*vtbl).GetBufferPointer)(error_blob) as *const i8;
                let buf_len = ((*vtbl).GetBufferSize)(error_blob);
                let slice = std::slice::from_raw_parts(buf_ptr as *const u8, buf_len);
                let msg = String::from_utf8_lossy(slice).to_string();
                let _ = ((*vtbl).parent.Release)(error_blob);
                msg
            }
        } else {
            format!("D3DCompile failed with HRESULT 0x{:08X}", hr as u32)
        };
        unsafe { FreeLibrary(module) };
        return Err(err_msg);
    }

    if !error_blob.is_null() {
        unsafe {
            let vtbl = *(error_blob as *mut *mut ID3D10BlobVtbl);
            let _ = ((*vtbl).parent.Release)(error_blob);
        }
    }

    let bytecode = unsafe {
        let vtbl = *(code_blob as *mut *mut ID3D10BlobVtbl);
        let buf_ptr = ((*vtbl).GetBufferPointer)(code_blob) as *const u8;
        let buf_len = ((*vtbl).GetBufferSize)(code_blob);
        let slice = std::slice::from_raw_parts(buf_ptr, buf_len);
        let code = slice.to_vec();
        let _ = ((*vtbl).parent.Release)(code_blob);
        code
    };

    unsafe { FreeLibrary(module) };
    Ok(bytecode)
}
