# Gameplay HUD 리디자인 계획 (2026-10-03)

## 배경

상업 리듬액션게임 수준 UI/UX를 목표로 전체 화면을 순차 리디자인한다.
엔진 제약(AGENTS.md 불변식): tiny-skia + softbuffer 소프트웨어 렌더링,
bitmap font만 사용, GPU 셰이더/파티클 라이브러리 금지. 표현 수단은
`draw_rect` 알파 블렌딩, `components/mod.rs`의 `render_panel` /
`render_card` / `render_badge` / `render_progress_bar` / `render_header_bar`
조합으로 제한된다. 새 크레이트 도입 없이 기존 컴포넌트 라이브러리를
각 화면에 일관되게 적용하는 것이 핵심 작업.

## 중요 발견: 렌더링 경로가 두 갈래다

`beetle-app`은 Windows에서 D3D11이 활성화되면 게임플레이 화면을
`crates/beetle-render/src/screens/gameplay_gpu.rs`
(`render_gameplay_gpu`, `SpriteBatcher` 기반)로 그린다. 소프트웨어 렌더러
경로(`crates/beetle-render/src/screens/gameplay.rs`, `render_gameplay`)는
D3D11 비활성 시 또는 일시정지 중에만 쓰인다
(`crates/beetle-app/src/main.rs`의 `is_gpu_gameplay` 분기 참고).
**즉 게임플레이 화면 비주얼을 바꾸려면 두 파일을 모두 고쳐야 실제로
눈에 보인다.** 소프트웨어 경로만 고치고 끝내면 Windows 실행 바이너리에서는
변화가 전혀 나타나지 않는다 (이번 세션에서 실제로 겪은 삽질).

`SpriteBatcher`는 바인딩된 텍스처가 바뀔 때마다(솔리드 rect = 텍스처 없음,
텍스트 = 폰트 아틀라스 텍스처) GPU draw call을 플러시한다.
`test_render_gameplay_gpu_batched_draw_calls`가 프레임당 1~3 draw call을
강제하므로, GPU 경로에서 rect 배경과 텍스트를 번갈아 그리면(인터리빙)
이 테스트가 깨진다. rect는 전부 모아서 먼저 그리고 텍스트는 모아서 나중에
그리는 "2-패스" 구조를 지켜야 하며, 기존 함수가 이미 그 순서
(rect 다발 → 텍스트 다발)로 짜여 있으므로 새 UI 요소를 중간에 끼워 넣을
자리가 마땅치 않다. 이번 라운드는 배경 박스를 포기하고 **텍스트 전용**
레이아웃 개선(캡션+값 분리, 2열 그리드, 색상 구분)으로 범위를 줄여
이 제약을 피했다.

## 화면별 체크리스트 (진행 순서: 사용자 지정)

- [x] Gameplay HUD (이번 세션 완료)
  - [x] 소프트웨어 경로(`gameplay.rs::draw_hud_info`): 평문 텍스트 →
        `render_badge`/`render_card` 컴포넌트 기반 카드/배지 레이아웃
  - [x] GPU 경로(`gameplay_gpu.rs`): 캡션+값 분리, BPM/LV 통합 한 줄,
        JUDGE BREAKDOWN 2열 그리드 (배경 박스는 배칭 제약으로 보류)
  - [ ] 노트/judge line 비주얼 고도화 (범위 밖, 추후 세션)
  - [ ] GPU 경로에 배경 박스를 넣으려면 함수 전체의 rect/text 패스
        순서를 재구성해야 함 (추후 세션 별도 작업으로 분리 권장)
- [ ] SongSelect 다듬기 (버튼/배지 질감, 선택 행 강조)
- [ ] Result 화면
- [ ] KeyConfig 화면
- [ ] Modals

## Gameplay HUD 상세 변경

### 소프트웨어 경로 (`gameplay.rs::draw_hud_info`)

기존: `BitmapFont::draw_text` 를 위→아래로 반복 호출하는 평문 리스트.
배경/구분/강조 없음 — 판정 중 시선이 분산됨.

변경 후 레이아웃 (우측 사이드 패널, playfield_y 기준 240*s 높이 예산 안):
1. 타이틀 (shadow) + 아티스트 — 유지, 폰트만 기존 그대로
2. BPM / LEVEL — 2분할 `render_badge` 행
3. EX SCORE / ACCURACY — 2분할 `render_badge` 행 (강조 색상: Yellow / Cyan)
4. PACEMAKER(AAA) — 전체 폭 `render_badge`, 양수/음수에 따라 Green/Red
5. JUDGE BREAKDOWN — `render_card` 컨테이너 안에 2열 x 3행 그리드,
   각 셀은 등급 색상 틴트 배경(카운트 비례 폭) + 라벨/카운트 텍스트

### GPU 경로 (`gameplay_gpu.rs`)

같은 정보 구조를 텍스트 전용으로 재현 (배경 박스 없음 — 위 배칭 제약 참고):
1. 타이틀 + 아티스트 — 기존 유지
2. `BPM {val}   LV {val}` 한 줄 통합
3. EX SCORE / ACCURACY / PACEMAKER — 각각 작은 캡션 줄 + 큰 값 줄
   (`stat_line!` 매크로로 라벨/값 2줄 패턴 통일)
4. JUDGE BREAKDOWN — 캡션 + 2열 x 3행 텍스트 그리드 (`col_w` 고정 오프셋)

## components/mod.rs 관련 버그 수정 (부수 발견)

`crates/beetle-render/src/lib.rs`에 `pub mod components;` 선언이
누락되어 있어 `render_panel`/`render_card`/`render_badge` 등 컴포넌트
라이브러리 전체가 어디서도 컴파일되지 않는 죽은 코드였다. 이번 세션에서
모듈 선언을 추가하고, 그 때까지 한 번도 컴파일된 적이 없어 숨어있던
타입 오류(`f32 * u32`, `if/else` 분기 타입 불일치) 몇 건을 함께 고쳤다.

## 검증 (완료)

- `cargo build --release -p beetle-app` 성공
- `cargo test -p beetle-render --release`: 41/41 통과
  (`test_render_gameplay_gpu_batched_draw_calls` 포함)
- `computer_use`로 실제 플레이 화면 캡처 확인 — GPU 경로 HUD가
  캡션/값 분리 + 색상 구분 + JUDGE BREAKDOWN 2열 그리드로 정상 렌더링됨
- `cargo fmt -p beetle-render` 적용
