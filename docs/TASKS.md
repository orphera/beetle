# TASKS.md — Beetle 로드맵 및 개발 체크리스트 (Milestone 10)

이 문서는 Beetle 프로젝트의 활성 마일스톤 구현 태스크를 관리하는 로드맵 문서입니다.

> 💡 **이전 마일스톤 완료 내역 아카이브**:
> - [archive/tasks_milestone_1.md](archive/tasks_milestone_1.md): 기반 아키텍처, 오디오 엔진, 패키지 포맷 및 1차 게임 루프
> - [archive/tasks_milestone_2.md](archive/tasks_milestone_2.md): UI/UX 전면 개편, 다국어 폰트, 인게임 일시정지, 결과 보상 화면, 1:1 키 리바인딩
> - [archive/tasks_milestone_3.md](archive/tasks_milestone_3.md): 아키텍처 모듈화 & 클린 구조 리팩토링 (`beetle-render` 및 `beetle-app` 서브모듈화)
> - [archive/tasks_milestone_4.md](archive/tasks_milestone_4.md): BMS Package Delta(차분) 및 원자적 업데이트 엔진 (`.bmdp` 빌더, 패치 및 GUI 마법사)
> - [archive/tasks_milestone_5.md](archive/tasks_milestone_5.md): 디스플레이 다원화, 종횡비 보존 렌더링, 초고주사율 최적화, BGA 엔진 및 WMF 동영상 지원
> - [archive/tasks_milestone_6.md](archive/tasks_milestone_6.md): 초경량 멀티 백엔드 GPU 하드웨어 가속 렌더링 엔진, CJK 한자 폴백, 진성 GPU 배치 파이프라인
> - [archive/tasks_milestone_7.md](archive/tasks_milestone_7.md): 듀얼 아틀라스(Sound & BGA Atlas) 초고속 패키지 엔진, 무손실 FLAC 압축, WebDAV VFS 온더플라이 WAV 합성
> - [archive/tasks_milestone_8.md](archive/tasks_milestone_8.md): 원격 패키지 레지스트리 및 온라인 송 허브 (`bpm update/search/install/upgrade`, `bpm serve`, `bpm-gui` 온라인 탭)
> - [archive/tasks_milestone_9.md](archive/tasks_milestone_9.md): 순수 .bmsp 아카이브 전용 저장소 및 UI 디자인 시스템 수립

---

# 🚀 Milestone 10: 멀티키 모드 실동작 지원 (Multi-Key Mode Playability: 9K / 10K / 14K)

자세한 기술 설계 및 아키텍처는 [specs/multi_key_mode_support.md](specs/multi_key_mode_support.md)를 참조합니다.

**배경**: 9Key(PMS)/10Key/14Key(더블 플레이) 차트는 메타데이터 탐지만 정상이고, 실제로는 2P/추가 키 채널이 전부 배경음악으로 치환되어 5Key·7Key 범위로만 플레이된다. `Lane` enum, 파서, 입력 설정이 모두 "7K + 1S" 전용으로 하드코딩되어 있던 것이 근본 원인.

---

## 📋 Phase 1: 코어 데이터 모델 — Lane enum 확장 및 2-패스 모드 인식 파서 (`crates/beetle-core`)
- [x] **`Lane` enum에 신규 변형 추가 (append-only, 기존 8개 값/순서 불변)**
  - [x] `Key8`, `Key9` (PMS 9K 전용 추가 버튼)
  - [x] `P2Scratch`, `P2Key1`..`P2Key7` (DP 10K/14K 2P 측)
- [x] **파서를 PASS 2a(플래그 스캔) / PASS 2b(노트 생성)로 분리**
  - [x] PASS 2a: 전체 측정 라인을 순회하며 `has_scratch`/`has_k67`/`has_2p_dp`/`has_pms_ch`/`has_2p_key1` 플래그만 먼저 확정 (`scan_measure_flags`)
  - [x] PASS 2a 종료 후 `chart.detect_play_mode()`로 최종 `PlayMode` 확정
  - [x] PASS 2b: 확정된 모드를 `channel_to_lane(ch, mode)`에 전달하여 노트 생성
- [x] **채널 21~29/61~69 모드별 분기 매핑**
  - [x] `Keys9`: `22→Key6, 23→Key7, 24→Key8, 25→Key9` (+LN 62~65)
  - [x] `Keys10`/`Keys14`: `21→P2Key1..25→P2Key5, 26→P2Scratch, 28→P2Key6, 29→P2Key7` (+LN 61~69)
  - [x] 그 외 모드(변칙 5K/7K)는 기존처럼 `bgm_notes` 폴백 유지 (회귀 방지)
- [x] **모디파이어(`modifier.rs`) 안전 통과**: `Key1..Key7` 외 레인(Key8/Key9/P2*)은 셔플 대상에서 제외하고 identity 통과 (SRandom 분기도 `KEY_LANES` 외 레인은 건너뛰도록 수정)
- [x] **리플레이(`replay.rs`) 직렬화 확장**: 신규 Lane 값을 8번부터 append, 기존 `.rep` 파일 역호환 유지
- [x] **단위 테스트**: 9K(`test_pms_9k_extra_buttons_are_judgeable_key6_to_9`)/10K(`test_2p_dp_notes_are_now_judgeable_not_bgm`)/14K(`test_dp_14k_all_2p_lanes_and_long_notes_judgeable`) 픽스처로 모든 2P/추가 채널 노트가 올바른 Lane으로 생성되는지 검증
- [x] **컴파일 전파 수정**: `beetle-render`(`lane_index`/`key_pressed` 배열을 `LANE_COUNT=18`로 확장, `skin.rs`의 `lane_x`/`lane_color`/`key_beam_color`에 신규 레인 임시 배치 추가)와 `beetle-app`(`input.rs`의 `lane_to_name`/`name_to_lane` 전체 레인 지원, 7K+1S 전용 프리셋은 신규 레인에 `"None"` 반환)의 exhaustive match 컴파일 에러 전부 해소. `cargo check/test --workspace` 및 `cargo build --release` 통과 확인.

> 참고: Phase 1 완료를 위해 `beetle-render`의 `lane_x()`에 DP 2P 레인의 **임시** 배치(1P 플레이필드 바로 뒤에 이어 붙이는 방식)를 추가했다. 이는 컴파일/판정 정합성만 보장하는 자리표시자이며, 실제 좌/우 듀얼 플레이필드 레이아웃은 Phase 3에서 교체한다.

---

## 📋 Phase 2: 판정/스코어 엔진 정합성 (`crates/beetle-core/src/judge`)
- [x] `Lane` exhaustive match 컴파일 에러 전부 해소 (현재 8개 가정 코드 전수 점검) — `judge/mod.rs`, `score_tracker.rs` 둘 다 `Lane` 값을 동등 비교로만 다루고 고정 배열/exhaustive match가 전혀 없어 추가 수정이 필요 없음을 코드 전수 확인으로 검증. `beetle-audio`도 `Lane`을 전혀 참조하지 않음을 확인.
- [x] `JudgeEngine`/`ScoreTracker`가 신규 레인 노트를 1P 7K와 동일하게 판정하는지 확인 — 신규 테스트 `test_judge_engine_handles_dp_14k_lanes_with_correct_max_score`에서 `Lane::P2Key1` 노트를 1P 레인과 동일하게 PerfectGreat로 판정함을 확인
- [x] 신규 레인 포함 차트의 EX Score/정확도 분모가 실제 판정 가능 노트 수와 일치하는지 테스트로 검증 — 14K 차트(1P 1개 + 2P 2개 = 3개 실제 노트)의 `max_ex_score()`가 정확히 6(3노트×2점)임을 검증, BGM 폴백으로 인한 왜곡 없음

---

## 📋 Phase 3: 렌더 레이아웃 — 9K 단일 플레이필드 / DP 듀얼 플레이필드 (`crates/beetle-render`)
- [ ] `SkinConfig::update_layout`에 `Keys9` 전용 분기 (스크래치 없는 9레인 단일 플레이필드)
- [ ] DP(10K/14K)용 2차 좌표 세트 추가 (`playfield_x_p2` 등) 및 `lane_x()`의 `P2*` 레인을 Phase 1의 임시 배치에서 실제 좌/우 듀얼 플레이필드 좌표로 교체
- [ ] 화면 좌/우 듀얼 플레이필드 레이아웃 + HUD/BGA 축소 재배치
- [x] `lane_index()` 및 `key_pressed` 고정 배열을 `[bool; LANE_COUNT]`(18)로 확장 (`renderer.rs`, `gameplay_gpu.rs`) — Phase 1에서 선반영
- [ ] 소프트웨어 렌더러(`gameplay.rs`)·GPU 렌더러(`gameplay_gpu.rs`) 양쪽 동일 레이아웃 적용

---

## 📋 Phase 4: 입력 설정 & Key Config UI (`crates/beetle-app`)
- [ ] `KeyPreset`에 9K/DP 전용 신규 프리셋 추가 (기존 `HomeRow`/`ArcadeZx`는 7K+1S 전용으로 불변 유지)
- [ ] `InputConfig`가 모드별 기본 바인딩 세트를 제공하도록 확장
- [ ] `main.rs`가 Key Config 화면에 현재 로드된 차트의 `PlayMode`에 맞는 전체 레인 목록을 전달하도록 수정 (`skin.active_lanes()` 고정 호출 제거)
- [ ] Key Config 화면 UI: 레인 수가 8개를 넘는 DP 모드를 위한 2열 레이아웃 또는 스크롤 지원

---

## 📋 Phase 5: 전체 워크스페이스 검증 및 릴리스 빌드
- [ ] `cargo check --workspace` 타입 체크
- [ ] `cargo test --workspace` 전체 테스트 통과 검증
- [ ] 9K/10K/14K 실제 BMS 패키지로 수동 플레이 검증 (노트 판정, 키 입력, 레이아웃)
- [ ] `cargo build --release` 바이너리 크기 및 정상 동작 확인

---

## 🎨 UI 디자인 시스템
- [specs/ui_design_system.md](specs/ui_design_system.md): 타이포그래피 4단계 위계, 색상 의미 체계, 테두리 사용 정책(보더 다이어트), 리스트/모달 그룹핑 컨벤션. 화면 UI를 고칠 때 이 문서를 기준으로 삼는다.

## 🔭 향후 확장 제안 및 백로그 (Future Proposals & Backlog)
- [proposals/legacy_compatibility_vfs.md](proposals/legacy_compatibility_vfs.md): 레거시 구동기(LR2/beatoraja) 하위 호환을 위한 무설치 WebDAV VFS 마운트 및 FUSE 확장 제안서.
- [proposals/platform_expansion.md](proposals/platform_expansion.md): Linux 네이티브 데스크톱 지원, WebAssembly(WASM/Web Audio) 무설치 웹 플레이어/뷰어, 모바일/태블릿 터치 제스처 지원 제안서.
- [proposals/decoupled_bga_package_system.md](proposals/decoupled_bga_package_system.md): 오디오/차트 코어 패키지와 대용량 BGA 패키지의 완전 분리 및 온디맨드 결합 제안서.
- [proposals/gameplay_enhancement_and_display.md](proposals/gameplay_enhancement_and_display.md): 플레이 옵션(스피드, 판정 오프셋) 및 디스플레이 고도화 제안서.
