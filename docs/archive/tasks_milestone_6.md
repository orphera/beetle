# TASKS.md — Beetle 로드맵 및 개발 체크리스트 (Milestone 6 Archive)

이 문서는 완료된 Beetle Milestone 6 개발 태스크 아카이브입니다.

---

# 🚀 Milestone 6: 초경량 멀티 백엔드 GPU 하드웨어 가속 렌더링 엔진 (Lightweight Multi-Backend GPU Acceleration)

자세한 기술 설계 및 아키텍처는 [proposals/lightweight_gpu_acceleration.md](../proposals/lightweight_gpu_acceleration.md)를 참조합니다.

---

## 📋 Phase 1: 2D 배치 렌더러 및 초경량 GPU HAL 인터페이스 설계 (`crates/beetle-render/`)
- [x] **초경량 `GpuBackend` 트레이트 정의 (`crates/beetle-render/src/backend/mod.rs`)**
  - [x] 2D 리듬게임에 특화된 6개 핵심 API 추상화 (`create_texture`, `update_texture`, `destroy_texture`, `draw_batch`, `resize`, `begin_frame` / `end_frame`)
  - [x] 정점 데이터 포맷 `Vertex2D` (`position: [f32; 2]`, `uv: [f32; 2]`, `color: [f32; 4]`) 정의
  - [x] 블렌딩 모드 `BlendMode` (`Alpha`: 기본 알파 블렌딩, `Additive`: 판정 빔/레인 가산 혼합)
- [x] **2D Sprite / Quad Batcher 구축 (`crates/beetle-render/src/backend/batcher.rs`)**
  - [x] 텍스처 단위 버텍스/인덱스 2D 쿼드 자동 배칭 (단 1~3회의 DrawCall로 전체 UI/노트 일괄 출력)
  - [x] 다국어 비트맵 폰트 아틀라스 텍스처 업로드 및 일괄 렌더링 지원
- [x] **안전한 CPU 소프트웨어 폴백 백엔드 (`SoftBackend`) 구현**
  - [x] 기존 `tiny-skia` 기반 렌더러를 `GpuBackend` 구현체로 래핑하여 무중단 폴백 보장

---

## 📋 Phase 2: Windows 네이티브 Direct3D 11 백엔드 구현 (`crates/beetle-render/src/backend/d3d11/`)
- [x] **Zero-Crate OS 네이티브 Direct3D 11 / DXGI COM 바인딩**
  - [x] 외부 무거운 크레이트(`wgpu`, `ash` 등) 없이 Windows 표준 시스템 DLL(`d3d11.dll`, `dxgi.dll`) 직접 연동
  - [x] `D3D11CreateDeviceAndSwapChain` 저지연 플립 스왑체인(`DXGI_SWAP_EFFECT_FLIP_DISCARD`) 초기화
- [x] **사전 컴파일 셰이더 바이트코드 임베딩**
  - [x] 런타임 셰이더 컴파일러(`D3DCompile`) 배제 및 사전 컴파일된 미니멀 2D CSO 바이트코드 바이너리 임베딩
  - [x] 알파 블렌딩 및 가산 블렌딩용 `ID3D11BlendState` 구성
  - [x] 텍스처 샘플러(`ID3D11SamplerState`) 바이리니어 및 포인트 필터링 지원
- [x] **GPU 디바이스 소실(Device Lost / Reset) 자동 복구**
  - [x] `DXGI_ERROR_DEVICE_RESET` / `DEVICE_REMOVED` 감지 시 자원 자동 재성성 또는 `SoftBackend`로 투명한 전환

---

## 📋 Phase 3: BGA & 동영상 하드웨어 텍스처 스트리밍 최적화
- [x] **BGA 이미지 & 동영상 프레임 고속 VRAM 업로드**
  - [x] BMS `#BMPxx` 이미지 시퀀스를 GPU 텍스처 풀로 적재
  - [x] WMF Video Player의 RGB32 프레임을 동적 텍스처(`update_texture`)로 저지연 스트리밍
- [x] **인게임 비주얼 이펙트 GPU 하드웨어 가속**
  - [x] 판정선 타격 빔 및 레인 이퀄라이저 가산 블렌딩(`BlendMode::Additive`) GPU 가속
  - [x] 종횡비 보존 뷰포트(`ImageFitMode`) 정점 UV 매핑 하드웨어 처리

---

## 📋 Phase 4: 게임 엔진 및 화면 상태 통합 (`crates/beetle-app/`)
- [x] **런타임 그래픽 백엔드 자동 감지 및 선택**
  - [x] 앱 기동 시 D3D11 하드웨어 가속 시도 -> 실패 시 `SoftBackend` CPU 폴백
  - [x] `config.dat`에 `gpu_backend` 설정(`Auto`, `Direct3D11`, `Software`) 추가 및 지속 저장
- [x] **옵션 모달(`Tab`) 렌더러 전환 지원**
  - [x] 인게임 및 곡 선택 화면에서 렌더러 상태(D3D11 / Soft) 인디케이터 표시
  - [x] 옵션 모달에서 실시간 렌더러 백엔드 전환 기능 제공
- [x] **전 기능 회귀 테스트 & 바이너리 크기 검증**
  - [x] 전체 워크스페이스 65개 이상 단위/통합 테스트 무결성 검증
  - [x] 바이너리 크기 다이어트 목표 준수 (< 1.2 MB: beetle-app 1.15 MB, bpm-gui 1.15 MB, bpm 570 KB)

---

## 📋 Phase 5: CJK 한자 런타임 폴백 렌더링 (`crates/beetle-render/src/bitmap_font/`)
- [x] **Windows GDI FFI 런타임 글리프 래스터화 계층 구축 (`crates/beetle-render/src/bitmap_font/gdi_fallback.rs`)**
  - [x] 외부 크레이트 0개, Windows 내장 `gdi32.dll` 직접 FFI 바인딩 (`CreateCompatibleDC`, `CreateFontW`, `GetGlyphOutlineW`)
  - [x] `GGO_GRAY8_BITMAP` 8bpp 안티에일리어싱 글리프 비트맵 추출 및 0..255 정규화
- [x] **렌더 스레드 전용 글리프 캐시 및 5단계 폴백 체인 통합 (`BitmapFont::draw_char`)**
  - [x] 1: ASCII 5x7 -> 2: 한글 10x8 -> 3: 가나/특수기호 10x8 -> 4: (신규) GDI 런타임 캐시 -> 5: 네모 박스 폴백
  - [x] `HashMap<char, Option<GlyphBitmap>>` 기반 성공/실패 양방향 캐싱 (중복 GDI 호출 방지)
  - [x] 고속 정수 알파 블렌딩 AA 블릿 함수 (`blit_glyph_aa`) 구현
  - [x] 비Windows 조건부 컴파일(`#[cfg(target_os = "windows")]`) 및 5단계 네모 박스 안전 폴백 검증

---

## 📋 Phase 6: 진성 GPU 가속 파이프라인 및 고성능 렌더링 최적화 (True GPU Batched Pipeline & Caching)
- [x] **선곡 화면 및 UI 상태 기반 Dirty 렌더링 캐싱 (`crates/beetle-app/`)**
  - [x] 사용자 입력(커서 이동, 검색어 변경, 모달 등)이 없는 대기 프레임 감지 (`is_dirty`)
  - [x] 정적 프레임 시 CPU 소프트웨어 래스터화 완전 생략 및 GPU 텍스처 풀 업로드 바이패스
  - [x] 유휴(Idle) 상태 CPU 점유율 및 불필요한 VRAM 버스 대역폭 0% 달성
- [x] **인게임 정적 플레이필드 & HUD 백버퍼 캐싱 (`crates/beetle-render/src/screens/gameplay.rs`)**
  - [x] 곡 시작 및 뷰포트 변경 시 플레이필드 배경, 레인 분할선, 고정 HUD 라벨을 정적 Pixmap에 1회 사전 렌더링
  - [x] 매 프레임 `clear()` + 전체 재드로우 대신 고속 슬라이스 복사 후 동적 요소(노트, 마디선, 콤보, 판정)만 렌더링
  - [x] CPU 게임플레이 렌더링 루프 연산량 40% 이상 절감
- [x] **인게임 진성 GPU 하드웨어 배치 렌더링 파이프라인 구축 (`render_gameplay_gpu`)**
  - [x] 내장 비트맵 폰트(ASCII 5x7, 볼드 숫자 8x12)를 128x128 텍스처 아틀라스로 베이크하는 `FontAtlas` 구현
  - [x] `SpriteBatcher` 기반 게임플레이 전용 GPU 렌더링 함수 구현:
    - [x] 마디선, 노트, 롱노트, 플레이필드: Untextured Quad 배치
    - [x] 키빔, 판정선 글로우, 히트 버스트: `BlendMode::Additive` 하드웨어 가산 혼합 배치
    - [x] 콤보 숫자, 판정 텍스트: Font Atlas UV 매핑 `draw_sub_sprite` 배치
    - [x] BGA / 비주얼라이저: 동적 텍스처 스프라이트 배치
  - [x] 단 1~3회의 `GpuBackend::draw_batch`로 게임플레이 프레임 완결 (CPU 래스터화 0%)
  - [x] `D3d11Backend` 하드웨어 가속 및 `SoftBackend` CPU 폴백 양방향 호환성 보장
- [x] **BMSP 패키지 내 무디스크 인메모리 비디오 재생 지원 (`crates/beetle-app/src/loader.rs`)**
  - [x] 임시 디스크 추출 없이 메모리 바이트 스트림(`VideoSource::Memory`)으로부터 직접 WMF 비디오 로드 및 재생
  - [x] 비정상 종료 시에도 디스크 잔여 임시 파일 0개 보장
