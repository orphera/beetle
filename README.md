# Beetle (BMS + Little) 🪲

Beetle은 바이너리 크기 최소화와 단일 실행 파일 구동을 최우선으로 설계된 초경량 Rust 기반 BMS(Be-Music Source) 플레이어입니다.

---

## 🎯 설계 철학 (우선순위 순서)

1. **가벼울 것 (Ultra Lightweight)**: 특히 **바이너리 크기를 최우선 지표**로 봅니다. 불필요한 의존성을 철저히 배제하고 실용적인 구조를 지향합니다.
2. **All Batteries Included**: 외부 코덱, 런타임, GPU 셰이더 컴파일러, 임베디드 데이터베이스 설치 없이 단일 실행 파일(`.exe`) 하나로 즉시 구동됩니다.
3. **UX & 판정 정확도**: 프레임레이트 변동에 영향을 받지 않는 절대적 판정 정확도와 높은 시각적 반응성을 제공합니다.

---

## 🏛️ 핵심 아키텍처 원칙

- **오디오 클럭 마스터 (Audio Clock Master)**: 판정 기준 시간은 프레임 타이머가 아니라 오디오 하드웨어가 실제로 재생한 샘플 수(`AudioClock`)를 단일 진실 공급원(Single Source of Truth)으로 삼습니다.
- **오디오 스레드 락프리 & 무할당 (Zero-Allocation & Lock-Free)**: 오디오 콜백 스레드는 뮤텍스 락을 걸거나 힙 할당을 절대 수행하지 않습니다. 키음 트리거는 실시간 SPSC 링버퍼(`rtrb`)를 통해 비동기 전달됩니다.
- **사전 PCM 디코딩 (Pre-decoded PCM Soundbank)**: 모든 키음(WAV/OGG)은 채보 로드 시점에 메모리에 PCM으로 완전히 디코드해 둡니다. 플레이 도중 런타임 디코딩을 일체 수행하지 않습니다.
- **3-스레드 분리 모델**:
  1. **오디오 스레드**: `cpal` 오디오 스트림 및 저지연 PCM 믹싱 전용.
  2. **로직/입력 스레드**: 키보드 입력 수신, 판정 계산, 락프리 키음 트리거 큐잉.
  3. **렌더 스레드**: 오디오 클럭을 읽어 노트 위치를 계산하고 화면에 출력 (프레임레이트 독립적).

---

## 📦 v1 지원 범위

### 포함 (In Scope)
- **채보 파싱**: BMS / BME / BML 텍스트 포맷 (BPM 변화, `#STOP` 정지, 롱노트 기본 지원)
- **오디오 코덱**: WAV 필수 지원 (경량 내장 디코더), OGG Vorbis (`vorbis` feature flag)
- **오디오 엔진**: `cpal` 기반 자체 구현 저지연 믹서 (외부 믹서 라이브러리 미사용)
- **소프트웨어 2D 렌더링**: `tiny-skia` + `softbuffer` (GPU/wgpu/Vulkan/DirectX 런타임 의존성 제로)
- **폰트 시스템**: 벡터 폰트 래스터라이저 대신 내장 1비트/8비트 비트맵 폰트
- **스킨**: 직관적인 미니멀 단일 설정 포맷 (좌표/색상)
- **곡 관리 & 스코어**: 폴더 스캔 메타데이터 캐시 및 로컬 플랫 파일 스코어 저장

### 제외 (Out of Scope for v1)
- BGA (영상/애니메이션 배경)
- LR2 스킨 포맷 호환
- 인터넷 랭킹 (IR) 및 네트워크 기능
- 리플레이 저장/재생
- 난이도표 연동
- MP3 / FLAC 코덱

---

## 📁 프로젝트 구조

```
beetle/
├── Cargo.toml                # Workspace 설정 및 release 크기 최적화 프로필
├── AGENTS.md                 # 아키텍처 불변식, 코딩 표준, 의존성 정책
├── docs/
│   ├── TASKS.md              # Phase별 개발 체크리스트
│   └── DECISIONS.md          # 아키텍처 결정 레코드 (ADR)
└── crates/
    ├── beetle-core/          # 순수 채보 파서, 타이밍 모델, 판정 엔진 (GUI/오디오 비의존)
    ├── beetle-audio/         # cpal 오디오 엔진, 락프리 믹서, 오디오 클럭
    ├── beetle-render/        # tiny-skia + 내장 비트맵 폰트 2D 소프트웨어 렌더러
    └── beetle-app/           # 바이너리 진입점, winit + softbuffer 창 및 루프
```

---

## 🛠️ 빌드 및 실행

```bash
# 개발 빌드 확인
cargo check

# 단위 테스트 실행
cargo test

# 바이너리 크기 최적화 릴리스 빌드
cargo build --release

# 실행
cargo run -p beetle-app --release
```
