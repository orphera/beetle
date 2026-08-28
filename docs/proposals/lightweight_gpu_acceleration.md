# 초경량 멀티 백엔드 GPU 하드웨어 가속 렌더링 엔진 제안서 (Lightweight Multi-Backend GPU Acceleration)

본 문서는 Beetle 리듬 게임 엔진의 렌더링 성능을 극대화하고 CPU 부하를 제거하기 위해, **Direct3D 11, OpenGL/GLES/WebGL, Vulkan, Metal** 및 **소프트웨어 폴백(	iny-skia)**을 지원하는 **초경량 GPU 하드웨어 추상화 계층(HAL: Hardware Abstraction Layer)**을 설계하는 향후 마일스톤 6 제안서입니다.

---

## 1. 배경 및 필요성 (Background & Motivation)

### 🚨 CPU 소프트웨어 렌더링(	iny-skia + softbuffer)의 한계
1. **메모리 대역폭 한계 및 CPU 부하**:
   - 고해상도(1080p, 1440p, 4K) 및 초고주사율(144Hz, 240Hz, 360Hz+) 환경에서 매 프레임 수백만 픽셀의 버퍼를 CPU 메모리에서 갱신하고 OS 프레임버퍼로 복사(Blit)하는 과정에서 상당한 CPU 점유율과 발열이 발생합니다.
2. **동영상 및 실시간 BGA 시퀀스 부하**:
   - 곡 플레이 중 수백 장의 고해상도 BGA 이미지 시퀀스나 동영상 프레임을 CPU에서 소프트웨어 블릿/알파 블렌딩할 때 프레임 드롭 위험이 존재합니다.
3. **2D 비주얼 이펙트 확장 한계**:
   - 판정선 폭타 빔, 레인 글로우(Glow), 가산 블렌딩(Additive Blending), 스캔라인/비네팅 후처리 이펙트를 CPU 연산만으로 처리하기에는 렌더링 타임 버짓(144Hz 기준 6.9ms)이 빠듯합니다.

### 🎯 설계 핵심 원칙: 초경량 & 독립 실행 불변식 유지
- **거대 GPU 프레임워크(wgpu) 배제**: SPIR-V/WGSL 런타임 셰이더 컴파일러가 포함된 수십 MB 단위의 프레임워크를 도입하지 않고, 2D 리듬게임에 특화된 미니멀 바인딩을 적용합니다.
- **사전 컴파일 셰이더 임베딩**: 런타임 셰이더 컴파일러 의존성을 100% 제거하고 사전 컴파일된 바이트코드(D3D11 CSO, Vulkan SPV) 및 미니멀 GLSL/MSL 텍스트를 바이너리에 직접 내장합니다.
- **안전한 소프트웨어 폴백 유지**: GPU 드라이버가 없거나 하드웨어 가속 초기화 실패 시 기존 	iny-skia 소프트웨어 렌더러로 투명하게 전환(Fallback)됩니다.

---

## 2. 아키텍처 설계 (Architecture Design)

`mermaid
graph TD
    subgraph beetle-app (게임 로직 & UI 계층)
        Logic[Game Loop / SongSelect / Gameplay / Result / KeyConfig]
    end

    subgraph beetle-render (하드웨어 가속 2D 코어)
        Batcher[2D Sprite / Quad Batcher<br/>(단 1~3회의 DrawCall로 화면 일괄 렌더링)]
        TexAtlas[Texture Pool & Dynamic Streamer<br/>(자켓, BGA 프레임, 비트맵 폰트)]
        HAL[trait GpuBackend<br/>(초경량 그래픽 추상화 인터페이스)]
    end

    subgraph Pluggable Backends (모듈형 드라이버)
        D3D11[backend_dx11<br/>(Windows 기본, Zero-Crate OS D3D11/DXGI)]
        GL[backend_gl<br/>(Linux / WebGL2 / Android, 경량 FFI)]
        VK[backend_vulkan<br/>(현대적 저오버헤드 Vulkan API)]
        MTL[backend_metal<br/>(macOS / iOS Native Metal)]
        SOFT[backend_soft<br/>(기존 tiny-skia, 안전한 CPU 폴백)]
    end

    Logic --> Batcher
    Batcher --> TexAtlas
    Batcher --> HAL
    HAL --> D3D11
    HAL --> GL
    HAL --> VK
    HAL --> MTL
    HAL --> SOFT
`

---

## 3. 핵심 모듈 및 인터페이스 설계

### 3.1 초경량 GpuBackend 트레이트 (crates/beetle-render/src/backend/mod.rs)

2D 리듬게임 렌더링에 필수적인 6개의 핵심 기능만 추상화하여, 백엔드별 구현체 코드를 200~400줄 수준으로 극도로 간결하게 유지합니다.

`ust
pub type TextureId = u32;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex2D {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Alpha,    // 일반 알파 블렌딩
    Additive, // 판정 폭타, 레인 빔 가산 혼합
}

pub trait GpuBackend {
    /// 정적 텍스처 생성 (자켓, 비트맵 폰트 아틀라스 등)
    fn create_texture(&mut self, width: u32, height: u32, pixels: &[u8]) -> TextureId;

    /// 동적 텍스처 갱신 (BGA 프레임, 동영상 스트리밍 등)
    fn update_texture(&mut self, id: TextureId, pixels: &[u8]);

    /// 텍스처 메모리 해제
    fn destroy_texture(&mut self, id: TextureId);

    /// 버텍스/인덱스 2D 쿼드 일괄 드로우 콜
    fn draw_batch(
        &mut self,
        vertices: &[Vertex2D],
        indices: &[u16],
        texture: Option<TextureId>,
        blend_mode: BlendMode,
    );

    /// 뷰포트 및 스왑체인 크기 조절
    fn resize(&mut self, width: u32, height: u32);

    /// 프레임 버퍼 스왑 (V-Sync / Fast-Sync)
    fn present(&mut self, vsync: bool);
}
`

---

## 4. 단계별 구현 로드맵 (Phased Implementation Plan)

### 📋 Phase 1: GpuBackend HAL 및 Windows Direct3D 11 백엔드 구축
- [ ] eetle-render 내부 ackend/ 추상화 모듈 구축 (	rait GpuBackend)
- [ ] ackend_dx11: Windows OS 내장 Direct3D 11 장치 생성, 스왑체인, 직교 투영 파이프라인 구현
- [ ] 2D 기본 셰이더(HLSL) 사전 컴파일 바이트코드(shader.cso) 임베딩
- [ ] 기존 소프트웨어 렌더러(ackend_soft)를 폴백 백엔드로 캡슐화

### 📋 Phase 2: 2D 인스턴스/스프라이트 배치 렌더러 구현
- [ ] 단일 정점 버퍼에 노트, 판정 라인, 빔, 비트맵 폰트를 배치(Batching)하여 1회의 DrawCall로 렌더링
- [ ] 비트맵 글리프 아틀라스 및 자켓 이미지 GPU 텍스처 풀 적재
- [ ] 알파 블렌딩 및 판정 빔/폭타 가산 블렌딩(Additive Blending) 파이프라인 분기

### 📋 Phase 3: BGA 실시간 하드웨어 믹서 및 동적 텍스처 스트리밍
- [ ] #BMPxx 이미지 시퀀스용 GPU 텍스처 풀 사전 적재
- [ ] update_texture를 통한 BGA 프레임 고속 버퍼 갱신
- [ ] POOR 오버레이 및 BGA 뷰포트 종횡비 보존 하드웨어 스케일링

### 📋 Phase 4: 크로스 플랫폼 백엔드 확장 (OpenGL, Vulkan, Metal)
- [ ] ackend_gl: Linux 및 WASM(WebGL2)용 경량 OpenGL 드라이버 연동
- [ ] ackend_vulkan: 저지연 고성능 Vulkan 파이프라인 구축
- [ ] ackend_metal: Apple 생태계 대응 Metal 백엔드 추가
- [ ] config.json에 ender_backend: auto | dx11 | vulkan | opengl | software 설정 지원
