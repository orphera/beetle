# Beetle (BMS + Little) 🪲

Beetle은 **바이너리 크기 최소화(< 1MB)**와 **단일 실행 파일 구동**을 최우선으로 설계된 초경량 Rust 기반 BMS(Be-Music Source) 플레이어 및 에코시스템입니다.

---

## 🎯 핵심 철학 (Core Philosophy)

1. **초경량 & 바이너리 크기 최우선 (Ultra Lightweight)**: 런타임 최적화와 함께 **바이너리 크기를 최우선 지표**로 봅니다. 불필요한 의존성을 철저히 배제하고 정적 컴파일을 지향합니다.
2. **독립 실행 (All Batteries Included)**: 외부 코덱 팩, C++ 런타임 DLL, GPU 셰이더 컴파일러, 임베디드 데이터베이스 설치 없이 단일 실행 파일(`.exe`)로 즉시 구동됩니다.
3. **결정론적 판정 (Deterministic Judgement)**: 프레임레이트 변동이나 렉에 영향을 받지 않도록 오디오 하드웨어 샘플 클럭을 단일 진실 기준으로 삼아 판정 오차가 누적되지 않습니다.
4. **쾌적한 반응성 (Non-Blocking UI)**: 무거운 파일 I/O와 오디오 디코딩을 백그라운드 Worker 스레드로 위임하여 60 FPS 무중단 화면을 보장합니다.

---

## 🏛️ 아키텍처 원칙

- **오디오 클럭 마스터 (Audio Clock Master Time)**: 판정(`JudgeEngine`) 및 렌더링 노트 위치는 오디오 하드웨어가 실제로 재생한 샘플 수(`AudioClock`)를 기준으로 계산됩니다.
- **오디오 스레드 락프리 & 무할당 (Zero-Allocation & Lock-Free)**: 오디오 콜백 스레드는 뮤텍스 락이나 힙 메모리 할당을 일체 수행하지 않으며, 실시간 SPSC 링버퍼(`rtrb`)를 통해 커맨드를 수신합니다.
- **사전 PCM 디코딩 (Pre-decoded PCM Soundbank)**: 모든 키음(WAV/OGG)은 로딩 시점에 메모리에 PCM으로 완전히 디코드해 둡니다. 플레이 도중 런타임 디코딩을 일체 수행하지 않습니다.
- **소프트웨어 2D 렌더링 (No Heavy GPU Dependencies)**: `tiny-skia` + `softbuffer` 기반으로 구동되어 GPU 드라이버 호환성 이슈 없이 수백 FPS 이상을 부드럽게 렌더링합니다.

---

## ✨ 지원 기능 (Feature Matrix)

| 구분 | 지원 내용 |
| :--- | :--- |
| **채보 포맷** | BMS, BME, BML, PMS (BPM 변화, `#STOP`, LNOBJ/LNType1 롱노트, 배속 변경) |
| **플레이 모드** | **5키 / 7키 / 9키 / 14키(DP)** 채보 자동 판별 및 가변 플레이필드 레이아웃 |
| **오디오 엔진** | `cpal` 기반 자체 저지연 믹서, WAV 디코더, OGG Vorbis (`lewton`) 내장 |
| **시각 효과** | 8방향 방사형 스파크 히트 버스트, 콤보 펄스 애니메이션, PGREAT 네온 빔, 16밴드 실시간 비주얼라이저 |
| **플레이 옵션** | Hi-Speed (0.5x ~ 10.0x), 판정 오프셋(ms), 레인 커버, 서든, 배치 옵션(Mirror, Random, R-Random, S-Random), 게이지(Groove, Survival, EX-Hard, Easy) |
| **보조 기능** | 리플레이 녹화/재생 (`.rep`), 연습 모드(마디 패스트포워드), 오토플레이 (`[A]`), 키 설정 UI (`[F12]`) |
| **패키지 생태계** | 표준 `.bmsp` 패키지 포맷(`bms-package`), 패키지 매니저(`bpm`), 독립형 데스크톱 GUI(`bpm-gui`) |
| **로딩 시스템** | 전용 로딩 화면(`AppScreen::Loading`), 비동기 키음 적재, Windows 콘솔 창 은닉 (`windows_subsystem`) |

---

## 📁 프로젝트 및 워크스페이스 구조

```text
beetle/
├── Cargo.toml                      # Workspace 및 release 최적화 프로필
├── AGENTS.md                       # 개발 가이드 및 아키텍처 불변식
├── docs/
│   ├── TASKS.md                    # 마일스톤 개발 체크리스트 및 백로그 (Milestone 4/5)
│   ├── DECISIONS.md                # 아키텍처 결정 레코드 (ADR-001 ~ ADR-017)
│   ├── specs/                      # 세부 기술 명세서
│   │   ├── bms_package_system.md   # 통합 BMS 패키지 시스템 아키텍처 명세
│   │   ├── bms_package.md          # bms-package 포맷 및 라이브러리 명세
│   │   └── bms_package_manager.md  # bms-package-manager 및 레지스트리 명세
│   ├── proposals/                  # 향후 확장 제안서 및 백로그
│   │   ├── gameplay_enhancement_and_display.md # 창모드, 종횡비 피팅, 초고주사율, BGA 제안서
│   │   ├── platform_expansion.md   # Linux, Web(WASM), Mobile 확장 제안서
│   │   ├── remote_package_registry.md # 원격 레지스트리 및 P2P 공유 제안서
│   │   └── legacy_compatibility_vfs.md # LR2/beatoraja VFS 호환 제안서
│   └── archive/                    # 완료된 마일스톤 히스토리 아카이브
│       ├── tasks_milestone_1.md    # Milestone 1: 기반 아키텍처 및 게임 루프
│       ├── tasks_milestone_2.md    # Milestone 2: UI/UX 전면 개편 및 다국어 폰트
│       ├── tasks_milestone_3.md    # Milestone 3: 모듈화 & 클린 구조 리팩토링
│       └── tasks_milestone_4.md    # Milestone 4: BMS Package Delta(차분) 및 원자적 업데이트 엔진
└── crates/
    ├── beetle-core/                # 순수 채보 파서, 타이밍 모델, 판정/점수/리플레이 엔진
    ├── beetle-audio/               # cpal 오디오 엔진, 락프리 믹서, 마스터 오디오 클럭
    ├── beetle-render/              # tiny-skia + 내장 비트맵 폰트 2D 소프트웨어 렌더러
    ├── beetle-app/                 # Beetle 메인 게임 (winit + softbuffer, 논블로킹 UI)
    ├── bms-package/                # .bmsp 패키지, .bmdp 차분(Delta) 포맷, 결정론적 패커
    ├── bms-package-manager/        # 패키지 수명주기, registry.json, 원자적 업데이트, bpm CLI
    └── bpm-gui/                    # 독립형 경량 데스크톱 패키지 매니저 GUI (차분 마법사 내장)
```

---

## 📊 바이너리 크기 벤치마크 (Release Profile)

`opt-level = "z"`, `lto = true`, `panic = "abort"`, `strip = true` 최적화를 적용한 단일 정적 바이너리 크기입니다:

| 바이너리 | 설명 | 파일 크기 |
| :--- | :--- | :--- |
| **`beetle-app.exe`** | Beetle 메인 BMS 플레이어 | **~1.03 MB (Release)** |
| **`bpm-gui.exe`** | 독립형 GUI 패키지 매니저 | **~1.04 MB (Release)** |
| **`bpm.exe`** | 패키지 관리자 CLI 도구 | **~0.60 MB (Release)** |

---

## 🎮 인게임 기본 조작키

### 선곡 화면 (Song Select)
- **`↑` / `↓` / `K` / `J`**: 1곡 단위 선택 이동
- **`PageUp` / `PageDown`**: 10곡 단위 빠른 점프 이동
- **`Home` / `End`**: 곡 목록 맨 처음 / 맨 끝으로 즉시 이동
- **`Enter` / `Space`**: 곡 플레이 시작 (전용 로딩 화면 거쳐 진입)
- **`Tab` / `O`**: 플레이 옵션 모달 열기/닫기 (배속, 게이지, 배치, 시작 마디)
- **`A`**: AutoPlay 모드 토글
- **`R`**: 저장된 리플레이 실행
- **`F2`**: 정렬 모드 변경 (제목순 / 레벨순 / 클리어마크순 / 스코어순)
- **`F3` / `F4`**: Hi-Speed 배속 증감 (0.25x 단위)
- **`F6` / `F7`**: 게이지 타입 (Groove / Easy / Hard / Hazard) / 배치 모디파이어 순환
- **`F12` / `C`**: 키 설정(Key Config) 화면 진입
- **`F5`**: 곡 목록 및 스테이지 이미지 새로고침

### 게임플레이 (Gameplay)
- **기본 7K 키 배치 (1P Standard - Arcade ZX)**:
  - `LShift`: Scratch
  - `Z`, `S`, `X`, `D`, `C`, `F`, `V`: 1 ~ 7번 레인
- **홈로우 키 배치 (HomeRow)**:
  - `LShift`: Scratch
  - `S`, `D`, `F`, `Space`, `J`, `K`, `L`: 1 ~ 7번 레인
- **`1` / `2` 또는 `PageUp` / `PageDown`**: 인게임 실시간 배속(Hi-Speed) 증감
- **`F10` / `F11`**: 레인 커버 높이 조절
- **`Esc`**: 일시정지(Pause) 모달 호출 (재개 / 재시작 / 선곡창 복귀 선택)
- **폭사 규칙**: `Hard` / `Hazard` 게이지 선택 시 게이지가 0이 되면 즉시 음악이 멈추며 `STAGE FAILED` 결과 화면으로 전환

---

## 📦 패키지 매니저 도구 사용법

### 1. 독립형 GUI 매니저 (`bpm-gui`)
```bash
# GUI 매니저 실행
cargo run -p bpm-gui --release
```
- **`↑` / `↓` / `K` / `J`**: 설치된 패키지 목록 탐색
- **`[/]`**: 실시간 곡명/ID/아티스트 검색 필터
- **`←` / `→`**: 다중 버전 선택
- **`[A]`**: 선택한 버전을 활성 버전으로 지정
- **`[U]` / `[Delete]`**: 선택한 버전 언인스톨
- **`[I]` / `[F1]`**: 기존 BMS 폴더 경로 입력 시 원클릭 패킹 & 설치
- **`[P]`**: BMS 폴더를 표준 `.bmsp` 아카이브로 내보내기
- **`[F2]`**: `.bmsp` 파일 직접 설치
- **`[C]` / `[F4]`**: 차분(Delta, `.bmdp`) 제작 마법사 모달
- **`[D]` / `[F3]`**: 차분 패키지(`.bmdp`) 원클릭 패치 및 원자적 업데이트

### 2. CLI 도구 (`bpm`)
```bash
# 1. BMS 폴더를 .bmsp 파일로 패킹
bpm pack ./songs/my_song/ -o my_song-1.0.0.bmsp

# 2. 기존 BMS 폴더를 패키지 관리자로 즉시 임포트 & 설치
bpm import ./songs/my_song/

# 3. 로컬 .bmsp 패키지 파일 설치
bpm install ./my_song-1.0.0.bmsp

# 4. 차분(Delta, .bmdp) 생성
bpm diff base-1.0.0.bmsp target-1.1.0.bmsp -o patch-1.1.0.bmdp
# 또는 새 채보 폴더에서 Base 패키지 기준으로 자동 차분 생성
bpm pack ./songs/my_song_v2/ --base base-1.0.0.bmsp -o patch-1.1.0.bmdp

# 5. 차분 패치 적용 및 원자적 업데이트
bpm patch base-1.0.0.bmsp patch-1.1.0.bmdp -o target-1.1.0.bmsp
# 또는 설치된 패키지에 1-명령어로 즉시 업데이트
bpm update patch-1.1.0.bmdp

# 6. 설치된 활성 패키지 목록 조회
bpm list

# 7. 패키지 상세 정보 및 버전 목록 확인
bpm info <package_id>

# 8. 활성 버전 전환
bpm activate <package_id> <version>

# 9. 패키지 버전 삭제
bpm uninstall <package_id> <version>
```

---

## 🛠️ 빌드 및 테스트

```bash
# 워크스페이스 전체 타입 체크
cargo check --workspace

# 전체 69개 단위 테스트 실행
cargo test --workspace

# 크기 최적화 릴리스 빌드
cargo build --release
```
