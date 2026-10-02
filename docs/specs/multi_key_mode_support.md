# 멀티키 모드 실동작 지원 (Multi-Key Mode Playability: 9K / 10K / 14K)

## 1. 문제 정의

곡 선택 화면과 메타데이터 탐지(`BmsChart::detect_play_mode`)는 9Key(PMS/팝픈 스타일), 10Key(5+5 더블 플레이), 14Key(7+7 더블 플레이)를 정확히 라벨링하지만, 실제로는 **5Key와 7Key만 플레이 가능**하고 나머지는 조용히 하향 축소된다.

근본 원인은 3가지 레이어에 걸쳐 있다:

1. **데이터 모델**: `beetle-core::Lane` enum이 `Scratch, Key1..Key7` 8개뿐이라 2P(더블 플레이) 레인이나 9번째 키를 표현할 방법이 없다.
2. **파서**: `crates/beetle-core/src/bms.rs::parse_measure_line()`의 채널 21~29/61~69 처리가 무조건 `chart.bgm_notes`(배경음악, 판정 대상 아님)로 보내도록 하드코딩되어 있다. 주석에도 명시: `"routed to BGM keysounds in 1P mode"`.
3. **입력/렌더**: `beetle-app::input::InputConfig`는 주석부터 `"Key mapping presets for 7K + 1S play"`로 7K+스크래치 전용이고, `beetle-render::skin::SkinConfig`의 레이아웃 계산도 `Keys7 | Keys9 | Keys10 | Keys14`를 전부 동일하게 취급해 항상 7키 폭의 단일 플레이필드만 그린다. Key Config 화면도 `skin.active_lanes()`를 그대로 써서 모드와 무관하게 항상 최대 8레인만 보여준다.

## 2. 설계 결정

### 2.1 Lane enum 확장 (ADR 후보)

```rust
pub enum Lane {
    Scratch,
    Key1, Key2, Key3, Key4, Key5, Key6, Key7,  // 기존 7K 레인 (불변)
    Key8, Key9,                                 // PMS(9K) 전용 추가 버튼
    P2Scratch,
    P2Key1, P2Key2, P2Key3, P2Key4, P2Key5, P2Key6, P2Key7,  // DP(10K/14K) 2P 측
}
```

- 기존 8개 변형(`Scratch`..`Key7`)의 **순서와 의미는 절대 바꾸지 않는다** — 리플레이(`.rep`) 파일의 `Lane -> u8` 인코딩이 기존 값(0..7)에 의존하므로, 새 변형은 반드시 8번 값부터 이어 붙인다(append-only). 이미 저장된 5K/7K 리플레이의 하위 호환성을 깨지 않기 위한 불변식이다.
- `Key8`/`Key9`는 PMS(9K) 모드에서만 쓰인다. `P2*`는 DP(10K/14K) 모드에서만 쓰인다. 두 그룹은 동시에 쓰이지 않는다(PMS는 플레이어 2명 개념이 없고, DP는 9K가 아니다).

### 2.2 파서: 2-패스 채널 해석으로 전환

채널 `22`가 "PMS 6번 버튼"을 의미하는지 "DP 2P 2번 키"를 의미하는지는 **파일 전체를 봐야 결정**된다(`detect_play_mode()`가 이미 `has_pms_ch`, `has_2p_key1`, `has_k67`, `header.player` 조합으로 이를 구분하고 있음). 현재 파서는 헤더(PASS 1) → 노트(PASS 2) 2단계인데, 노트 파싱 중에 모드가 아직 확정되지 않은 상태로 채널 21~29를 처리하고 있어 항상 BGM으로 빠진다.

**해결**: PASS 2를 둘로 쪼갠다.
- **PASS 2a (플래그 스캔)**: 모든 측정 라인을 순회하며 `has_scratch`/`has_k67`/`has_2p_dp`/`has_pms_ch`/`has_2p_key1` 플래그만 갱신한다(노트는 아직 만들지 않음). 끝나면 `let mode = chart.detect_play_mode();`로 최종 모드를 확정한다.
- **PASS 2b (노트 생성)**: 확정된 `mode`를 `parse_measure_line(..., mode)`에 전달한다. 채널 21~29/61~69의 해석을 모드에 따라 분기한다:
  - `PlayMode::Keys9`일 때: `22→Key6, 23→Key7, 24→Key8, 25→Key9` (62~65는 동일 레인의 LN). 이 모드에서는 `21/26/28/29`(및 LN 대응 `61/66/68/69`)가 애초에 나타나지 않아야 `Keys9`로 탐지되므로 처리하지 않는다.
  - `PlayMode::Keys10` 또는 `Keys14`일 때: `21→P2Key1, 22→P2Key2, 23→P2Key3, 24→P2Key4, 25→P2Key5, 26→P2Scratch, 28→P2Key6, 29→P2Key7` (6x는 LN 대응).
  - 그 외 모드(5K/7K인데 변칙적으로 2P 채널이 섞인 손상된 파일 등)는 기존 동작대로 `bgm_notes`로 폴백한다 — 회귀 방지를 위한 안전망.
- `channel_to_lane()`은 `fn channel_to_lane(ch: &str, mode: PlayMode) -> Option<Lane>`로 시그니처를 확장한다.

### 2.3 모디파이어(Random/Mirror/S-Random) 범위

`crates/beetle-core/src/modifier.rs`의 셔플 로직은 1P 7키(`Key1..Key7`)만 대상으로 설계되어 있다. 9K/DP용 셔플 알고리즘을 새로 설계하는 것은 범위 밖으로 두고, **`Key1..Key7`에 속하지 않는 레인(Key8/Key9/P2*)은 모디파이어를 그대로 통과(identity)시킨다.** 이것도 명시적 설계 결정이며, 추후 별도 제안서로 DP/9K 전용 모디파이어를 다룰 수 있다.

### 2.4 렌더링 레이아웃

- **9K (Keys9)**: 스크래치 없이 9개 레인을 한 줄로 배치하는 단일 플레이필드. `SkinConfig::update_layout`에 `Keys9` 전용 분기를 추가한다 (`scratch_lane_width` 제외, `9 * lane_width`).
- **10K/14K (DP)**: 1P 플레이필드와 2P 플레이필드를 화면 좌/우에 나란히 배치하는 **듀얼 플레이필드** 레이아웃이 필요하다. `SkinConfig`에 `playfield_x_p2`, `playfield_width_p2` 같은 2차 좌표 세트를 추가하거나, `lane_x(lane)`이 `P2*` 레인에 대해 화면 반대편 좌표를 반환하도록 확장한다. HUD(곡 정보/BGA)는 두 플레이필드 사이의 남는 공간에 축소 배치한다.
- `beetle-render::renderer::lane_index()`와 `key_pressed: [bool; 8]` 고정 배열(`renderer.rs`, `gameplay_gpu.rs`)을 `[bool; 18]`로 확장한다.

### 2.5 입력 설정 (`InputConfig`/`KeyPreset`) 및 Key Config 화면

- 기존 `KeyPreset::HomeRow`/`ArcadeZx`는 7K+1S 전용으로 유지한다(하위 호환, 가장 많이 쓰이는 모드이므로 변경 금지).
- 9K/DP용 신규 프리셋을 추가한다(가칭 `KeyPreset::Pms9K`, `KeyPreset::DoublePlay`). 커뮤니티 관행을 참고한 기본값 초안:
  - **DP(10K/14K)**: 1P `LShift + Z S X D C F V` (기존 ArcadeZx와 동일) + 2P `RShift + M , . / ...`(오른손 미러 배치). 정확한 키 배열은 구현 단계에서 사용자 피드백을 받아 조정한다.
  - **PMS(9K)**: 흔히 쓰이는 2단 레이아웃(예: 상단 Q W E R T, 하단 A S D F 또는 유사 컨벤션) — 구현 시 커뮤니티 표준을 조사해 확정한다.
- `crates/beetle-app/src/handlers/key_config.rs`와 `beetle-render::screens::key_config`는 현재 `skin.active_lanes()`(항상 ≤8개)를 그대로 쓰고 있어, 실제 로드된 차트의 `PlayMode`에 맞는 전체 레인 목록을 넘기도록 호출부(`main.rs`)를 수정해야 한다. DP처럼 레인이 16개가 되면 한 화면에 다 들어가지 않으므로 Key Config UI도 2열 레이아웃 또는 스크롤이 필요하다.

## 3. 구현 순서 (단계적, 각 단계마다 컴파일 가능 + 테스트 통과 유지)

1. **Phase 1 — 코어 데이터 모델** (`beetle-core`): Lane enum 확장, 2-패스 파서, `channel_to_lane(ch, mode)`, 모디파이어 identity 통과, 리플레이 직렬화 확장. 이 단계만으로 "9K/10K/14K 차트의 모든 노트가 최소한 판정 가능한 `NoteEvent`로 생성된다"를 보장한다(화면에 어떻게 배치할지는 Phase 3).
2. **Phase 2 — 판정/스코어 엔진 정합성** (`beetle-core::judge`): `JudgeEngine`이 신규 레인을 투명하게 처리하는지 확인(이미 `Lane`을 키로 쓰는 범용 로직이라 enum만 늘어나면 대부분 자동 대응되지만, exhaustive match 컴파일 에러를 전부 해소해야 한다).
3. **Phase 3 — 렌더 레이아웃** (`beetle-render`): 9K 단일 플레이필드, DP 듀얼 플레이필드 레이아웃, `lane_index`/`key_pressed` 배열 확장, HUD 재배치.
4. **Phase 4 — 입력 설정 & Key Config UI** (`beetle-app`): 신규 프리셋, Key Config 화면의 동적 레인 개수 지원(2열/스크롤).
5. **Phase 5 — 전체 워크스페이스 검증**: `cargo check/test --workspace`, 실제 9K/10K/14K 테스트 픽스처로 수동 플레이 검증, 릴리스 빌드.

각 Phase 종료 시 커밋한다 (AGENTS.md 컨벤션 — 작업 단위별 커밋).

## 4. 비목표 (Out of Scope, 이번 마일스톤에서 다루지 않음)

- 24K/24K-2P 같은 더 희귀한 모드 — BMS 생태계에서 비중이 낮아 후순위.
- DP/9K 전용 Random/Mirror 모디파이어 알고리즘 신규 설계 — 범위 밖, 추후 별도 제안서.
- 터치/게임패드 등 키보드 외 입력 장치 — `platform_expansion.md` 제안서 영역.
