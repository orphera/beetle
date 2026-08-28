# AGENTS.md — Beetle 개발 가이드 및 아키텍처 불변식

이 문서는 모든 AI 에이전트와 기여자가 작업 세션 시작 시 반드시 준수해야 하는 고정 아키텍처 규칙, 불변식, 의존성 정책, 코드 스타일 가이드라인입니다.

---

## 1. 핵심 철학 (Core Philosophy)

1. **바이너리 크기가 최우선 (Binary Size First)**: 런타임 최적화보다도 바이너리 크기 다이어트를 우선순위로 둡니다. 불필요한 크레이트 도입은 절대 금지합니다. (목표: 개별 실행 파일당 < 1 MB)
2. **독립 실행 (All Batteries Included)**: 외부 런타임(GPU 드라이버 셰이더 컴파일러, 코덱 팩, C++ 런타임 DLL) 의존 없이 단일 정적 바이너리로 구동되어야 합니다.
3. **결정론적 판정 (Deterministic Judgement)**: 오디오 하드웨어 샘플 클럭을 단일 진실 기준으로 삼아 프레임 드랍이나 렉이 발생해도 판정 오차가 누적되지 않아야 합니다.
4. **논블로킹 UI (Non-Blocking UI Experience)**: 무거운 파일 I/O나 오디오 디코딩 시 UI 이벤트 루프가 멈추지 않고 항상 부드러운 화면과 프로그레스를 유지해야 합니다.

---

## 2. 아키텍처 불변식 (Architectural Invariants)

이 규칙들은 어떤 리팩토링이나 기능 추가 시에도 절대 훼손되어서는 안 됩니다.

- **[INV-1] 오디오 클럭 마스터 (Audio Clock as Master Time)**
  - 판정(`JudgeEngine`) 및 렌더링 노트 위치 계산은 오직 `AudioClock::current_time_seconds()` / `current_samples()`만을 기준으로 합니다.
  - 렌더러의 `dt`나 OS 프레임 타이머로 노트를 전진시키지 않습니다.

- **[INV-2] 오디오 콜백 스레드 락프리 & 무할당 (Zero-Allocation & Lock-Free Audio Callback)**
  - `cpal` 오디오 콜백 함수 내부에서는 `Mutex`, `RwLock`, `std::sync::mpsc`, 채널 등 블로킹 동기화 객체를 사용할 수 없습니다.
  - 오디오 콜백 내부에서는 `Vec::new()`, `Box::new()`, `format!()` 등 어떠한 힙 할당도 발생해서는 안 됩니다.
  - 통신은 반드시 사전 할당된 SPSC 락프리 링버퍼(`rtrb`)로만 수행합니다.

- **[INV-3] 사전 PCM 디코딩 (Pre-decoded PCM Soundbank)**
  - 플레이 도중 WAV/OGG 파일 디코딩을 수행하지 않습니다. 곡 로딩 시점에 전체 키음을 메모리에 PCM 버퍼로 적재합니다.

- **[INV-4] 3-스레드 분리 모델**
  - **오디오 스레드**: 믹싱 및 `AudioClock` 누적 전용.
  - **로직/입력 스레드**: 키 입력 처리, 판정, 락프리 큐로 오디오 커맨드 전송.
  - **렌더 스레드**: `softbuffer` + `tiny-skia` 기반 프레임 그리기.

- **[INV-5] 비동기 백그라운드 I/O & 논블로킹 UI**
  - 대용량 파일 복사, 압축 해제, 디렉터리 패킹, 다수 키음 디코딩은 메인 UI 스레드를 블로킹하지 않고 백그라운드 Worker 스레드로 위임합니다.
  - UI는 `AppScreen::Loading` 또는 회전 스피너를 통해 60 FPS 무중단 상태를 유지합니다.

- **[INV-6] 패키징 바이트 단위 결정론 (Deterministic Packaging)**
  - `bms-package` 빌드 시 엔트리는 항상 사전식(Lexicographical)으로 정렬되며, 고정된 에포크 타임스탬프(`1980-01-01 00:00:00`)와 정규화된 JSON을 사용하여 동일 입력에 대해 언제나 바이트 단위로 동일한 `.bmsp` 아카이브를 생성합니다.

---

## 3. 의존성 정책 (Dependency Policy)

새로운 크레이트를 추가하기 전에는 반드시 대체 방안(표준 라이브러리 직접 구현)을 먼저 검토해야 합니다.

### 🚫 금지 라이브러리 목록 (Forbidden Crates)
- **GPU / 셰이더 관련**: `wgpu`, `vulkano`, `glow`, `glium`, `ash`, `pixels` (tiny-skia + softbuffer 유지)
- **무거운 오디오 라이브러리**: `rodio`, `kira`, `soloud` (cpal + 자체 믹서 유지)
- **무거운 파서/정규식**: `regex`, `nom`, `pest`, `combine` (BMS 파서는 순수 문자열 조작으로 작성)
- **폰트 래스터라이저**: `fontdue`, `freetype`, `rusttype`, `cosmic-text` (임베디드 비트맵 폰트 사용)
- **데이터베이스**: `sqlite`, `rusqlite`, `sled`, `rocksdb` (간단한 바이너리/텍스트 플랫 파일 포맷 사용)
- **무거운 직렬화**: 핫패스에서의 `serde_json` 남발 금지

### 허용된 핵심 크레이트
- `cpal` (오디오 I/O)
- `rtrb` (실시간 락프리 SPSC 링버퍼)
- `hound` (WAV 디코더)
- `lewton` (선택적 OGG Vorbis 디코더)
- `tiny-skia` (2D 소프트웨어 렌더링)
- `softbuffer` (네이티브 윈도우 프레임버퍼)
- `winit` (윈도우 및 이벤트)
- `zip` (패키지 컨테이너 - `deflate` 기능만 최소 활성화)
- `serde`, `serde_json` (패키지 Manifest 및 Registry 직렬화)

---

## 4. 모듈 책임 분리 원칙

- `crates/beetle-core`: 순수 알고리즘 크레이트로 OS API, 창, 오디오 하드웨어 의존성이 없습니다.
- `crates/beetle-audio`: cpal 기반 오디오 I/O, PCM 버퍼링, 락프리 믹서 및 마스터 클럭을 다룹니다.
- `crates/beetle-render`: tiny-skia 2D 소프트웨어 그래픽을 렌더링하며 입력을 직접 폴링하지 않습니다.
- `crates/beetle-app`: 게임 루프, 화면 상태 전이(`SongSelect`, `Loading`, `Gameplay`, `Result`, `KeyConfig`) 및 입력 통합을 담당합니다.
- `crates/bms-package`: 단일 패키지(`.bmsp`) 포맷, Manifest, 결정론적 패커 및 안전한 리더를 다룹니다.
- `crates/bms-package-manager`: 로컬 저장소(`packages/`), `registry.json`, 원자적 설치, 다중 버전 관리 및 `bpm` CLI를 담당합니다.
- `crates/bpm-gui`: 독립형 경량 데스크톱 패키지 관리 GUI 애플리케이션입니다.

---

## 5. 빌드 및 검증 명령

```bash
# 전체 워크스페이스 타입 체크
cargo check --workspace

# 전체 테스트 실행
cargo test --workspace

# 릴리스 빌드 (바이너리 크기 최적화)
cargo build --release

# 바이너리 파일 크기 확인 (PowerShell)
Get-Item .\target\release\beetle-app.exe, .\target\release\bpm-gui.exe, .\target\release\bpm.exe | Select-Object Name, Length
```

---

## 6. 문서 작성 및 링크 참조 규칙 (Documentation & Path Policy)

- **로컬 머신 절대 경로 노출 절대 금지**: 모든 Markdown 문서(`docs/`, `AGENTS.md`, `README.md` 등)에는 개발자 로컬 머신의 절대 경로(`C:/Users/...`, `file:///...`, `/home/...`, `/Users/...`)를 절대로 기재하거나 노출하지 않습니다.
- **상대 경로(Relative Path) 사용 원칙**: 저장소 내 파일 참조 시 반드시 프로젝트 루트 기준 상대 경로(예: `docs/specs/bms_package.md`, `crates/beetle-core/src/lib.rs`) 또는 마크다운 상대 링크(예: `[bms_package.md](specs/bms_package.md)`, `[tasks_milestone_1.md](archive/tasks_milestone_1.md)`)를 사용합니다.
