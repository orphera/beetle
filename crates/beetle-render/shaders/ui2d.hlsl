// Beetle 2D batch shaders (Direct3D 11, shader model 4.0 → feature level 10_0+).
//
// Compiled offline by scripts/compile-shaders.ps1 into shaders/compiled/*.cso,
// which are embedded with include_bytes! (no runtime d3dcompiler dependency).
// After editing this file, rerun the script and commit the .cso outputs; the
// `compiled_shaders_match_source` test fails until you do.

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
