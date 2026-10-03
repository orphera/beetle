//! Direct3D 11 2D batch shaders, embedded as offline-compiled bytecode.
//!
//! Source: `crates/beetle-render/shaders/ui2d.hlsl`. Rebuild the `.cso` files
//! with `scripts/compile-shaders.ps1` after editing it (ADR-026: no runtime
//! `d3dcompiler_47.dll` dependency).

pub const VS_MAIN: &[u8] = include_bytes!("../../../shaders/compiled/ui2d_vs.cso");
pub const PS_SPRITE: &[u8] = include_bytes!("../../../shaders/compiled/ui2d_ps_sprite.cso");
pub const PS_COLOR: &[u8] = include_bytes!("../../../shaders/compiled/ui2d_ps_color.cso");

#[cfg(test)]
mod tests {
    use super::*;

    /// DXBC container magic; every fxc output starts with it.
    const DXBC: &[u8] = b"DXBC";

    #[test]
    fn embedded_bytecode_is_dxbc() {
        for blob in [VS_MAIN, PS_SPRITE, PS_COLOR] {
            assert!(blob.len() > 32 && &blob[..4] == DXBC);
        }
    }

    /// Fails when ui2d.hlsl was edited without rerunning
    /// scripts/compile-shaders.ps1 (which snapshots the compiled source).
    #[test]
    fn compiled_shaders_match_source() {
        let normalize = |s: &str| s.replace("\r\n", "\n");
        let source = include_str!("../../../shaders/ui2d.hlsl");
        let stamp = include_str!("../../../shaders/compiled/ui2d.hlsl.stamp");
        assert_eq!(
            normalize(source),
            normalize(stamp),
            "shaders/ui2d.hlsl changed: run scripts/compile-shaders.ps1 and commit the .cso files"
        );
    }
}
