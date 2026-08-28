# TASKS.md — Beetle 로드맵 및 개발 체크리스트 (Milestone 4)

이 문서는 Beetle 프로젝트의 활성 마일스톤 구현 태스크를 관리하는 로드맵 문서입니다.

> 💡 **이전 마일스톤 완료 내역**:
> - [archive/tasks_milestone_1.md](archive/tasks_milestone_1.md): 기반 아키텍처, 오디오 엔진, 패키지 포맷 및 1차 게임 루프
> - [archive/tasks_milestone_2.md](archive/tasks_milestone_2.md): UI/UX 전면 개편, 다국어 폰트, 인게임 일시정지, 결과 보상 화면, 1:1 키 리바인딩
> - [archive/tasks_milestone_3.md](archive/tasks_milestone_3.md): 아키텍처 모듈화 & 클린 구조 리팩토링 (`beetle-render` 및 `beetle-app` 서브모듈화)

---

# 🚀 Milestone 4: BMS Package Delta(차분) 및 원자적 업데이트 엔진 (Delta & Atomic Update Engine)

Milestone 4의 목표는 [BMS Package System 표준 명세](specs/bms_package_system.md)와 [ADR-015](DECISIONS.md#adr-015-bms-패키지-차분delta-시스템-및-버전-전이-모델)에 따라, **BMS 작품의 대용량 재배포를 방지하고 차분 제작자/원곡자의 배포 마찰(Friction)을 제로화하는 고성능 차분(Delta) 생성·적용·원자적 업데이트 시스템**을 구축하는 것입니다.

핵심 원칙:
1. **결정론적 변환**: $\text{Apply}(\text{Package@base}, \text{Delta}(\text{base} \to \text{target})) = \text{Package@target}$
2. **원자적 안전성 (Atomic Safety)**: 패치 실패 시 기존 설치본 파괴 0% (Rollback & Full Package Fallback)
3. **제작자 마찰 제로 (Zero-Friction Creator UX)**: BMS 헤더 메타데이터 자동 추출 및 1-클릭/1-명령어 `.bmdp` 생성

---

## 📋 Phase 1: `bms-package` 차분 포맷 & Diff/Patch 코어 라이브러리 (`crates/bms-package/src/delta/`) (Completed)
- [x] **차분 메타데이터 모델 정의 (`delta/manifest.rs`)**
  - [x] `DeltaManifest`: `package_id`, `base_state_hash`, `target_state_hash`, `base_checksum`, `target_checksum`
  - [x] 엔트리 연산 분류: `added_resources`, `modified_resources`, `removed_resources`, `unchanged_resources`
  - [x] 매니페스트 직렬화/역직렬화 및 정규화
- [x] **결정론적 차분 빌더 (`delta/builder.rs`)**
  - [x] `DeltaBuilder`: Base 패키지와 Target 패키지 간의 리소스/차트 diff 추출
  - [x] 변경/추가된 리소스만 압축하여 `.bmdp` 아카이브 생성 (`INV-6` 결정론적 타임스탬프 및 사전순 정렬)
- [x] **차분 적용 및 타겟 재현 엔진 (`delta/applicator.rs`)**
  - [x] `DeltaApplicator`: `Base Package + Delta Archive` 검증 및 `Target Package` 완전 복원
  - [x] Base SHA-256 검증 및 생성된 Target SHA-256 일치 검증
- [x] **단위 테스트 스위트 작성**
  - [x] 차트만 추가된 케이스 (15 KB 초경량 차분)
  - [x] 키음 WAV 수정/추가/삭제 케이스
  - [x] Base 버전 불일치 및 손상된 Delta 거부 테스트

---

## 📋 Phase 2: `bms-package-manager` 원자적(Atomic) 업데이트 & 복구 엔진 (`crates/bms-package-manager/src/updater/`) (Completed)
- [x] **원자적 업데이트 파이프라인 (`updater.rs`)**
  - [x] 1단계: 설치된 패키지 state와 Base State 일치 확인
  - [x] 2단계: 임시 스테이징에서 Delta 적용 및 타겟 재현
  - [x] 3단계: 복원된 Target Package 무결성(SHA-256) 검증
  - [x] 4단계: 원자적 설치(`Atomic Commit`) 및 `registry.json` 버전 갱신
  - [x] 실패 시 기존 버전 100% 무손상 유지 및 롤백 보장
- [x] **Full Package Fallback 및 자동 복구 (Repair)**
  - [x] Base state 미설치/불일치 시 `BaseStateNotInstalled` 명확한 에러 전파 및 Full Package 수용 기반 마련

---

## 📋 Phase 3: `bpm` CLI 차분 명령어 및 제작자 툴링 (`crates/bpm/`) (Completed)
- [x] **`bpm diff` 서브커맨드**
  - [x] `bpm diff <base.bmsp> <target.bmsp> -o <diff.bmdp>`
  - [x] 디렉터리 기반 diff: `bpm diff <base_dir> <target_dir> -o <diff.bmdp>`
- [x] **`bpm patch` 서브커맨드**
  - [x] `bpm patch <base.bmsp> <diff.bmdp> -o <target.bmsp>`
- [x] **`bpm pack --base` 차분 제작자 지원 플래그**
  - [x] 새로 만든 채보 폴더에서 BMS 헤더(#TITLE, #ARTIST, #LEVEL) 자동 추출 후 Base 패키지와 묶어 즉시 `.bmdp` 생성
- [x] **`bpm update` 서브커맨드 연동**
  - [x] 로컬 차분 패키지를 감지하여 1-명령어 자동 원자적 업데이트

---

## 📋 Phase 4: `bpm-gui` 차분 제작 마법사 & 업데이트 UI 및 전체 회귀 검증 (`crates/bpm-gui/`) (Completed)
- [x] **`bpm-gui` 차분 제작 마법사 (Package & Delta Creator 모달/단축키)**
  - [x] Base 곡 / Target 폴더 입력 후 1-클릭 Delta(`.bmdp`) 빌드 (`[C]`/`F4`)
  - [x] 원본 곡 선택 및 차분 생성 파이프라인 연동
- [x] **1-클릭 패치 및 업데이트 UI**
  - [x] `.bmdp` 파일 드래그 앤 드롭 또는 단축키(`[D]`/`F3`) 시 자동 차분 패치 적용
  - [x] 백그라운드 Worker 스레드 + 회전 스피너 논블로킹 UI 연동
- [x] **전체 워크스페이스 회귀 검증 및 바이너리 크기 확인**
  - [x] `cargo test --workspace` (전체 69개 테스트 100% 통과)
  - [x] 바이너리 크기 < 1 MB 불변식 확인 (`beetle-app.exe: 863 KB`, `bpm-gui.exe: 943 KB`, `bpm.exe: 546 KB`)

---

# 🚀 Milestone 5 (Backlog): 디스플레이 다원화, 종횡비 보존 렌더링, 초고주사율 최적화 및 BGA 엔진

자세한 기술 설계 및 아키텍처는 [proposals/gameplay_enhancement_and_display.md](proposals/gameplay_enhancement_and_display.md)를 참조합니다.

## 📋 Phase 1: 디스플레이 및 창 모드 시스템 (Display & Window Mode Management)
- [ ] **창 모드 전환 기능 구현 (`crates/beetle-app/`)**
  - [ ] `Fullscreen (독점 전체화면)`: 모니터 최고 주사율 및 독점 제어
  - [ ] `Borderless Fullscreen (테두리 없는 전체화면)`: 데스크톱 해상도/주사율 유지 창 모드
  - [ ] `Windowed (창모드)`: 자유로운 윈도우 크기 조절 및 위치 이동
  - [ ] `Alt + Enter` 또는 옵션 모달(`Tab`)을 통한 실시간 즉시 전환
- [ ] **해상도 조절 및 가상 프레임버퍼 스케일러**
  - [ ] 가상 렌더링 해상도(720p, 1080p, 1440p, 4K) 선택 지원
  - [ ] 창 크기 변경 시 비율 유지 레터박스 또는 정수 배율(Integer Scaling) 출력
  - [ ] `config.json`에 `display_mode`, `window_width`, `window_height`, `target_fps` 지속 저장 및 복원

---

## 📋 Phase 2: 반응형 이미지 종횡비 보존 렌더링 (Aspect-Ratio Aware Image Fitting)
- [ ] **이미지 왜곡 방지 스케일러 구현 (`crates/beetle-render/src/image.rs`)**
  - [ ] `Fill & Center Crop`: 이미지의 원래 종횡비를 엄격하게 보존하며 대상 영역을 채우고 넘치는 영역을 중앙 기준 크롭
  - [ ] `Fit & Letterbox`: 원본 비율을 보존하여 전체 이미지를 표시하고 남는 여백에 다크 패딩/블러 적용
- [ ] **선곡 창 및 인게임 HUD 적용**
  - [ ] 선곡 창 자켓/배너 이미지 영역에서 4:3, 16:9, 1:1 이미지가 찌그러지지 않고 깔끔하게 렌더링되도록 적용
  - [ ] 인게임 $320 \times 180$ 스테이지/BGA 뷰포트에 종횡비 보존 크롭 적용

---

## 📋 Phase 3: 인게임 초고주사율 프레임 페이싱 & 렌더러 극한 최적화 (High-Refresh Rate Performance)
- [ ] **Windows 고정밀 프레임 타이머 연동**
  - [ ] Windows 멀티미디어 타이머 `timeBeginPeriod(1)` 적용으로 1ms 이하 슬립 정밀도 확보 및 마이크로 지터 제거
  - [ ] 144Hz / 240Hz / 360Hz+ 주사율 타겟 고정밀 프레임 페이서 구축
- [ ] **소프트웨어 렌더러 SIMD / 청크 최적화 (`crates/beetle-render/`)**
  - [ ] `draw_rect` 및 픽셀 블릿 루프 32-bit `u32` 청크/SIMD 병렬화
  - [ ] 정적 배경 및 HUD 영역 Dirty Rect 캐싱을 통한 불필요한 재렌더링 절감

---

## 📋 Phase 4: BGA (BackGround Animation) 시스템 지원 (BGA & Animated Stage Rendering)
- [ ] **BMS BGA 채널 파서 확장 (`crates/beetle-core/src/bms.rs`)**
  - [ ] `#BMPxx` 인덱스별 이미지 시퀀스 및 비디오 파일명 매핑 파싱
  - [ ] `#BGAxx` 좌표 슬라이스 및 투명색(ColorKey) 레이어 정의 파싱
  - [ ] BGA 이벤트 채널(04: Base, 06: Poor, 07: Layer) 타임라인 등록
- [ ] **실시간 BGA 믹서 및 애니메이션 렌더러 (`crates/beetle-render/`, `crates/beetle-app/`)**
  - [ ] 곡 로딩 시 BGA 이미지 시퀀스를 사전 메모리 텍스처 풀로 적재 (`INV-3` 준수)
  - [ ] `AudioClock` 시간에 동기화되어 $O(1)$로 현재 프레임 이미지를 인게임 BGA 뷰포트에 블릿
  - [ ] 미스/POOR 발생 시 POOR BGA 프레임 오버레이

---

## 🔭 향후 확장 제안 및 백로그 (Future Proposals & Backlog)
- [proposals/gameplay_enhancement_and_display.md](proposals/gameplay_enhancement_and_display.md): 디스플레이 다원화, 종횡비 보존 렌더링, 초고주사율 최적화 및 BGA 지원 제안서.
- [proposals/platform_expansion.md](proposals/platform_expansion.md): Linux 네이티브 데스크톱 지원, WebAssembly(WASM/Web Audio) 무설치 웹 플레이어/뷰어, 모바일/태블릿 터치 제스처 지원 제안서.
- [proposals/remote_package_registry.md](proposals/remote_package_registry.md): 원격 패키지 레지스트리, 1-클릭 다운로드/업데이트, 정적 CDN 호스팅, LAN P2P 공유 제안서.
- [proposals/legacy_compatibility_vfs.md](proposals/legacy_compatibility_vfs.md): 레거시 구동기(LR2/beatoraja) 하위 호환을 위한 무설치 WebDAV VFS 마운트 및 FUSE 확장 제안서.
