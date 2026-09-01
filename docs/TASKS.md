# TASKS.md — Beetle 로드맵 및 개발 체크리스트 (Milestone 6)

이 문서는 Beetle 프로젝트의 활성 마일스톤 구현 태스크를 관리하는 로드맵 문서입니다.

> 💡 **이전 마일스톤 완료 내역 아카이브**:
> - [archive/tasks_milestone_1.md](archive/tasks_milestone_1.md): 기반 아키텍처, 오디오 엔진, 패키지 포맷 및 1차 게임 루프
> - [archive/tasks_milestone_2.md](archive/tasks_milestone_2.md): UI/UX 전면 개편, 다국어 폰트, 인게임 일시정지, 결과 보상 화면, 1:1 키 리바인딩
> - [archive/tasks_milestone_3.md](archive/tasks_milestone_3.md): 아키텍처 모듈화 & 클린 구조 리팩토링 (`beetle-render` 및 `beetle-app` 서브모듈화)
> - [archive/tasks_milestone_4.md](archive/tasks_milestone_4.md): BMS Package Delta(차분) 및 원자적 업데이트 엔진 (`.bmdp` 빌더, 패치 및 GUI 마법사)
> - [archive/tasks_milestone_5.md](archive/tasks_milestone_5.md): 디스플레이 다원화, 종횡비 보존 렌더링, 초고주사율 최적화, BGA 엔진 및 WMF 동영상 지원

---

# 🚀 Milestone 6: 초경량 멀티 백엔드 GPU 하드웨어 가속 렌더링 엔진 (Lightweight Multi-Backend GPU Acceleration)

자세한 기술 설계 및 아키텍처는 [proposals/lightweight_gpu_acceleration.md](proposals/lightweight_gpu_acceleration.md)를 참조합니다.

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
  - [x] 바이너리 크기 다이어트 목표 준수 (< 1.2 MB: beetle-app 1.03 MB, bpm-gui 0.94 MB)

---

## 📋 Phase 5: CJK 한자 런타임 폴백 렌더링 (`crates/beetle-render/src/bitmap_font/`)
- [x] **Windows GDI FFI 런타임 글리프 래스터화 계층 구축 (`crates/beetle-render/src/bitmap_font/gdi_fallback.rs`)**
  - [x] 외부 크레이트 0개, Windows 내장 `gdi32.dll` 직접 FFI 바인딩 (`CreateCompatibleDC`, `CreateFontW`, `GetGlyphOutlineW`)
  - [x] `GGO_GRAY8_BITMAP` 8bpp 안티에일리어싱 글리프 비트맵 추출 및 0..255 정규화
- [x] **렌더 스레드 전용 글리프 캐시 및 5단계 폴백 체인 통합 (`BitmapFont::draw_char`)**
  - [x] 1: ASCII 5x7 -> 2: 한글 10x8 -> 3: 가나/특수기호 10x8 -> 4: (신규) GDI 런타임 캐시 -> 5: 네모 박스 폴백
  - [x] `HashMap<char, Option<GlyphBitmap>>` 기반 성공/실패 양방향 캐싱 (중복 GDI 호출 방지)
  - [x] 고속 정수 알파 블렌딩 AA 블릿 함수 (`blit_glyph_aa`) 구현
  - [x] 비Windows 조건부 컴파일(`#[cfg(target_os = "windows")]`) 및 5번 네모 박스 안전 폴백 검증

---

## 🔭 향후 확장 제안 및 백로그 (Future Proposals & Backlog)
- [proposals/gameplay_enhancement_and_display.md](proposals/gameplay_enhancement_and_display.md): 디스플레이 다원화, 종횡비 보존 렌더링, 초고주사율 최적화 및 BGA 지원 제안서 (Milestone 5 Archive).
- [proposals/lightweight_gpu_acceleration.md](proposals/lightweight_gpu_acceleration.md): 초경량 멀티 백엔드(D3D11, OpenGL, Vulkan, Metal, Software Fallback) GPU 하드웨어 가속 렌더링 엔진 제안서 (Milestone 6).
- [proposals/platform_expansion.md](proposals/platform_expansion.md): Linux 네이티브 데스크톱 지원, WebAssembly(WASM/Web Audio) 무설치 웹 플레이어/뷰어, 모바일/태블릿 터치 제스처 지원 제안서.
- [proposals/remote_package_registry.md](proposals/remote_package_registry.md): 원격 패키지 레지스트리, 1-클릭 다운로드/업데이트, 정적 CDN 호스팅, LAN P2P 공유 제안서.
- [proposals/legacy_compatibility_vfs.md](proposals/legacy_compatibility_vfs.md): 레거시 구동기(LR2/beatoraja) 하위 호환을 위한 무설치 WebDAV VFS 마운트 및 FUSE 확장 제안서.
