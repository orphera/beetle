# Beetle (BMS + Little) 🪲

Beetle은 **바이너리 크기 최소화(개별 실행 파일 1MB 미만 목표)**와 **단일 실행 파일 구동**을 최우선으로 설계된 Rust 기반 BMS(Be-Music Source) 플레이어와 패키지 관리 도구 모음입니다. Windows 전용이며, Direct3D 11을 OS DLL에 직접 연결해 그립니다.

---

## 🎯 핵심 철학 (Core Philosophy)

1. **바이너리 크기 최우선 (Binary Size First)**: 런타임 최적화보다 바이너리 크기를 우선합니다. 새 크레이트는 대체 방안(표준 라이브러리 직접 구현)을 먼저 검토한 뒤에만 들입니다.
2. **독립 실행 (All Batteries Included)**: 외부 코덱 팩, C++ 런타임 DLL, GPU 셰이더 컴파일러, 임베디드 데이터베이스 없이 단일 정적 실행 파일로 구동됩니다. MSVC 타깃은 정적 CRT로 링크합니다(`.cargo/config.toml`).
3. **결정론적 판정 (Deterministic Judgement)**: 프레임 변동이나 렉과 무관하게, 입력이 도착한 순간의 오디오 샘플 클럭을 판정 기준으로 삼습니다.
4. **논블로킹 UI (Non-Blocking UI)**: 대용량 파일 I/O, 압축 해제, 키음 디코딩은 백그라운드 스레드로 넘기고, 화면은 로딩 상태에서도 멈추지 않습니다.

---

## 🏛️ 아키텍처 원칙

[AGENTS.md](AGENTS.md)에 정리된 불변식(INV-1 ~ INV-6)을 따릅니다. 핵심만 요약하면 다음과 같습니다.

- **오디오 클럭 마스터 (INV-1)**: 판정과 노트 위치는 `AudioClock`의 재생 샘플 수만 기준으로 계산합니다. 렌더 `dt`나 OS 타이머로는 노트를 움직이지 않습니다.
- **락프리 오디오 콜백 (INV-2)**: 오디오 콜백은 뮤텍스나 힙 할당 없이 사전 할당된 SPSC 링버퍼(`rtrb`)로만 통신합니다.
- **사전 PCM 디코딩 (INV-3)**: 곡을 불러올 때 키음 WAV/OGG를 전부 PCM으로 디코드해 둡니다. 플레이 중에는 디코딩하지 않습니다.
- **3-스레드 분리 (INV-4, ADR-028)**: 오디오 스레드는 믹싱과 클럭 누적만 맡습니다. 입력/로직 스레드는 Raw Input으로 키의 도착 시각을 찍고 수동 플레이를 판정합니다. 렌더 스레드(winit)는 화면, IME, 메뉴 입력과 프레임 그리기를 맡습니다.
- **Direct3D 11 단일 렌더러 (ADR-026)**: OS 내장 Direct3D 11을 직접 FFI로 호출하고, 사전 컴파일된 셰이더와 공유 아틀라스로 그립니다. 하드웨어 어댑터가 없으면 WARP(Windows 내장 CPU 래스터라이저)로 같은 파이프라인을 실행합니다. 소프트웨어 렌더러는 폐지되었습니다.
- **결정론적 패키징 (INV-6)**: `.bmsp` 아카이브는 엔트리를 사전식으로 정렬하고 고정 타임스탬프를 써서, 같은 입력에서 항상 같은 바이트가 나옵니다.

---

## ✨ 지원 기능 (Feature Matrix)

| 구분 | 지원 내용 |
| :--- | :--- |
| **채보 포맷** | BMS, BME, BML, PMS (BPM 변화, `#STOP`, LNOBJ/LNType1 롱노트, 배속 변경) |
| **플레이 모드** | 4K / 5K / 6K / 7K / 8K / 9K(PMS) / 10K / 14K(DP) — 채보 선언(`#4K` 등)과 채널로 자동 판별 |
| **오디오 엔진** | `cpal` 기반 자체 락프리 믹서, WAV(`hound`), OGG Vorbis(`lewton`), FLAC(패키지 내) |
| **플레이 옵션** | Hi-Speed(그린 넘버 증감), 판정 오프셋 측정(캘리브레이션), 레인 커버, 배치 옵션(Mirror, Random, R-Random, S-Random), 게이지(Easy, Groove, Hard, Hazard) |
| **보조 기능** | 리플레이 녹화/재생, 오토플레이, 키 설정 화면(키보드·컨트롤러), 난이도표 연동, 검색·정렬·폴더 필터 |
| **화면 구성** | 타이틀, 선곡, 로딩, 플레이, 결과, 키 설정, 설정 |
| **패키지 생태계** | `.bmsp` 패키지 포맷(`bms-package`), 차분(`.bmdp`) 업데이트, 패키지 매니저 CLI(`bpm`), 독립형 GUI(`bpm-gui`), 원격 레지스트리와 LAN 공유(`bpm serve`) |

---

## 📁 프로젝트 및 워크스페이스 구조

```text
beetle/
├── Cargo.toml                      # 워크스페이스 및 release 최적화 프로필
├── AGENTS.md                       # 개발 가이드 및 아키텍처 불변식
├── docs/
│   ├── TASKS.md                    # 활성 마일스톤(Milestone 10) 체크리스트와 로드맵
│   ├── DECISIONS.md                # 아키텍처 결정 레코드 (ADR-001 ~ ADR-028)
│   ├── specs/                      # 세부 기술 명세서
│   │   ├── bms_package_system.md   # 통합 BMS 패키지 시스템 아키텍처
│   │   ├── bms_package.md          # bms-package 포맷 및 라이브러리
│   │   ├── bms_package_manager.md  # bms-package-manager 및 레지스트리
│   │   ├── pure_bmsp_storage.md    # 순수 .bmsp 아카이브 저장 방식
│   │   ├── remote_package_registry.md # 원격 패키지 레지스트리
│   │   ├── multi_key_mode_support.md  # 다중 키 모드(4K~14K) 지원
│   │   ├── controllers.md          # 컨트롤러 입력
│   │   └── ui_design_system.md     # UI 디자인 시스템
│   ├── plans/                      # 기능별 설계 문서 (날짜순)
│   ├── proposals/                  # 향후 확장 제안서 및 백로그
│   └── archive/                    # 완료된 마일스톤 기록 (Milestone 1 ~ 9)
└── crates/
    ├── bms-hash/                   # SHA-256, MD5 (의존성 없음). 차트 식별과 패키지 체크섬에 사용
    ├── beetle-core/                # 순수 알고리즘: 파서, 타이밍, 판정, 점수, 리플레이, 난이도표
    ├── beetle-audio/               # cpal 오디오 I/O, PCM 버퍼, 락프리 믹서, 마스터 클럭
    ├── beetle-render/              # Direct3D 11 2D 렌더러, 텍스트, 코드 생성 스킨, 화면(screens/)
    ├── beetle-app/                 # 메인 게임 (winit + Direct3D 11): 상태 전이, 입력, 로딩
    ├── bms-package/                # .bmsp 패키지, .bmdp 차분 포맷, 결정론적 패커
    ├── bms-package-manager/        # 패키지 수명주기, registry.json, 원자적 설치, bpm CLI
    └── bpm-gui/                    # 독립형 패키지 관리 GUI (차분 마법사, 온라인 탭 포함)
```

---

## 📊 바이너리 크기 (Release)

`opt-level = "z"`, `lto = true`, `panic = "abort"`, `strip = true`, 정적 CRT로 빌드한 크기입니다. 2026-10-18 로컬 빌드 기준이며, 릴리스마다 달라질 수 있습니다.

| 바이너리 | 설명 | 파일 크기 |
| :--- | :--- | :--- |
| **`beetle-app.exe`** | Beetle 메인 BMS 플레이어 | **~3.44 MB** |
| **`bpm-gui.exe`** | 독립형 GUI 패키지 매니저 | **~4.88 MB** |
| **`bpm.exe`** | 패키지 관리자 CLI 도구 | **~3.11 MB** |

현재 크기는 `<1 MB` 목표를 크게 넘습니다. 가장 큰 원인은 내장 서브셋 폰트(Latin, 조요 한자, 한글)로, 이는 의도된 결정입니다. 자세한 예산은 [AGENTS.md](AGENTS.md)의 의존성 정책을 참고하세요.

---

## 🎮 인게임 기본 조작키

아래 키는 기본값이며, 키 설정 화면(`F12` / `C`)에서 바꿀 수 있습니다.

### 타이틀
- 시작 시 곡 목록 읽기가 끝나면 한 번 나옵니다. 메뉴 항목은 키보드로 고릅니다.

### 선곡 화면 (Song Select)
- **`↑` / `↓` / `K` / `J`**: 한 곡씩 이동
- **`PageUp` / `PageDown`**: 10곡씩 이동
- **`Home` / `End`**: 목록 맨 처음 / 맨 끝
- **`←` / `→`**: 묶음(그룹) 행에서는 채보 전환, 그 밖에는 폴더 나가기 / 들어가기
- **`Enter` / `Space`**: 선택한 곡 시작 (로딩 화면을 거쳐 진입)
- **`Esc` / `Backspace`**: 상위 폴더로 이동 (최상위에서 `Esc`는 종료 확인)
- **`/`**: 검색창 열기 (IME 입력 지원)
- **`F10`**: 필터 행에 포커스 (`Esc` 또는 `F10`으로 해제)
- **`Tab` / `O`**: 플레이 옵션 패널 열기
- **`A`**: 오토플레이 토글
- **`R`**: 저장된 리플레이 재생
- **`F1` / `F3`**: 폴더 순환
- **`F2`**: 정렬 방식 변경
- **`F4`**: 설정 화면
- **`F5`**: 곡 목록 다시 읽기
- **`F12` / `C`**: 키 설정 화면
- **`?` / `H`**: 도움말 오버레이

### 게임플레이 (Gameplay)
- **기본 7K 키 배치 (ArcadeZX)**: `LShift`는 Scratch, `Z S X D C F V`는 1~7번 레인
- **홈로우 키 배치 (HomeRow)**: `LShift`는 Scratch, `S D F Space J K L`은 1~7번 레인
- **`F3` / `PageUp` / `1`**: 그린 넘버를 줄여 하이스피드를 높임 (노트가 더 빨리 흐름)
- **`F4` / `PageDown` / `2`**: 그린 넘버를 늘려 하이스피드를 낮춤
- **`F10` / `F11`**: 레인 커버 높이 증감
- **`Esc`**: 일시정지 (재개 / 재시작 / 선곡으로 복귀)
- **폭사 규칙**: `Hard` 또는 `Hazard` 게이지가 0이 되면 음악이 멈추고 `STAGE FAILED` 결과 화면으로 넘어갑니다.

---

## 📦 패키지 매니저 도구 사용법

### 1. 독립형 GUI 매니저 (`bpm-gui`)
```bash
# GUI 매니저 실행
cargo run -p bpm-gui --release
```
- **`↑` / `↓` / `K` / `J`**: 설치된 패키지 목록 탐색
- **`[/]`**: 곡명, ID, 아티스트로 검색 필터
- **`←` / `→`**: 다중 버전 선택
- **`[A]`**: 선택한 버전을 활성 버전으로 지정
- **`[U]` / `[Delete]`**: 선택한 버전 제거
- **`[I]` / `[F1]`**: BMS 폴더 경로를 입력해 패킹하고 바로 설치
- **`[P]`**: BMS 폴더를 `.bmsp` 아카이브로 내보내기
- **`[F2]`**: `.bmsp` 파일 직접 설치
- **`[C]` / `[F4]`**: 차분(`.bmdp`) 제작 마법사
- **`[D]` / `[F3]`**: 차분 패키지 패치 및 원자적 업데이트

### 2. CLI 도구 (`bpm`)
```bash
# 1. BMS 폴더를 .bmsp 파일로 패킹
bpm pack ./songs/my_song/ -o my_song-1.0.0.bmsp

# 2. 기존 BMS 폴더를 임포트해 설치
bpm import ./songs/my_song/

# 3. 로컬 .bmsp 패키지 파일 설치
bpm install ./my_song-1.0.0.bmsp

# 4. 차분(Delta, .bmdp) 생성
bpm diff base-1.0.0.bmsp target-1.1.0.bmsp -o patch-1.1.0.bmdp
# 또는 새 채보 폴더에서 Base 패키지 기준으로 바로 생성
bpm pack ./songs/my_song_v2/ --base base-1.0.0.bmsp -o patch-1.1.0.bmdp

# 5. 차분 패치 적용
bpm patch base-1.0.0.bmsp patch-1.1.0.bmdp -o target-1.1.0.bmsp
# 또는 설치된 패키지를 한 명령으로 업데이트
bpm update patch-1.1.0.bmdp

# 6. 설치된 활성 패키지 목록 조회
bpm list

# 7. 패키지 상세 정보와 버전 목록
bpm info <package_id>

# 8. 활성 버전 전환
bpm activate <package_id> <version>

# 9. 패키지 버전 제거
bpm uninstall <package_id> <version>

# 10. 원격 레지스트리 검색 / 설치 / 업그레이드
bpm search <keyword>
bpm upgrade

# 11. 로컬 네트워크로 패키지 공유
bpm serve
```

---

## 🛠️ 빌드 및 테스트

```bash
# 워크스페이스 전체 타입 체크
cargo check --workspace

# 전체 테스트 실행
cargo test --workspace

# 크기 최적화 릴리스 빌드
# .cargo/config.toml의 정적 CRT 설정을 지우지 마세요 (원칙 2).
cargo build --release

# 커밋 전 포맷 (AGENTS.md의 커밋 전 게이트)
cargo fmt --all
```

개발 문서와 작업 규칙은 [AGENTS.md](AGENTS.md)와 [docs/TASKS.md](docs/TASKS.md)를 참고하세요.
