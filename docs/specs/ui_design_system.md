# UI 디자인 시스템 (Beetle Bitmap UI) — Commercial Grade Refresh

> **상태와 권위**: 이 문서는 기존 UI 원칙과 상업급 디자인 리프레시 제안을 함께 담은 참고 문서다. 섹션 1–4는 구현 시 참고할 시각 설계 가이드이며 실제 적용 여부는 화면마다 다르다. 섹션 5–8의 토큰·컴포넌트·화면별 명세와 체크리스트는 미적용 제안을 포함하며, 현재 구현 규약으로 일괄 강제하지 않는다. 실제 변경은 [AGENTS.md](../../AGENTS.md)의 아키텍처 규칙과 `beetle-render`의 `theme.rs`, `components/`, `screens/` 구현을 기준으로 한다. 특히 Beetle 앱은 Windows D3D11 단일 렌더러와 WARP 폴백을 사용한다([ADR-026](../DECISIONS.md#adr-026-direct3d-11-단일-렌더러-채택-및-소프트웨어-렌더러-폐지-windows-전용)); 아래의 `SoftwareRenderer`, `tiny-skia`, GDI 관련 제안은 현재 렌더 아키텍처에 대한 지시가 아니다.

이 문서는 `beetle-render` 화면에 적용할 수 있는 **디자인 토큰(Color, Spacing, Typography, Shadow/Glow, Border Radius)**과 **컴포넌트 명세의 초안**을 기록합니다. 기존 "미니멀 일관성" 철학을 "상업 게임급 시각 계층 + 상태 피드백 + 컴포넌트 재사용"으로 확장하려던 제안입니다.

> **당시 구현 가정**: 이 제안은 폰트 래스터라이저 의존성을 추가하지 않고 비트맵 엔진과 Windows GDI 폴백을 가정했다. 현재 폰트·렌더 경로 설명은 [AGENTS.md](../../AGENTS.md)와 ADR-026을 따른다.

---

## 1. 타이포그래피 4단계 위계

모든 텍스트는 아래 4단계 중 하나에 속해야 합니다. `s`는 `Viewport::scale` (720p 기준 배율)입니다.

| 단계 | 용도 | 스케일 | 헬퍼 |
|---|---|---|---|
| **Title** | 화면/모달 제목, 선택된 곡 제목 | `(2.0 * s).round().max(1.0)` | `draw_text_with_shadow` 권장 |
| **Value** | 핵심 수치/본문 (BPM 값, EX Score, 라벨 값) | `(s * 0.9).round().max(1.0)` (= `font_scale`) | `draw_text` / `draw_bold_text` |
| **Label** | 보조 라벨 (박스 상단 캡션, 섹션 헤더) | `(font_scale * 8 / 10).max(1)` | `draw_text` |
| **Hint** | 하단 단축키 힌트, 각주 | `font_scale` 그대로 쓰되 명도만 낮춤 (별도 축소 스케일 불필요) | `draw_text` |

새 스케일 값을 임의로 추가하지 말고 이 4단계 중 하나를 재사용하세요. 스케일이 늘어날수록 화면마다 느낌이 달라집니다.

---

## 2. 색상 의미 체계

같은 RGB를 다른 의미로 쓰지 않습니다. 아래 역할별 팔레트를 그대로 재사용하세요.

### 2.1 텍스트 명도 사다리
| 역할 | 색상 | 사용처 |
|---|---|---|
| Primary (선택/강조 텍스트) | `(255,255,255)` | 선택된 행, 모달 제목 |
| Secondary (본문 텍스트) | `(190,195,215)` ~ `(160,170,190)` | 비선택 곡 제목, 일반 본문 |
| Tertiary (라벨) | `(120,130,160)` ~ `(140,150,175)` | 박스 캡션(`BPM`, `NOTES`), 보조 설명 |
| Hint (각주) | `(100,110,135)` ~ `(130,140,160)` | 하단 단축키 힌트 |

### 2.2 액센트 색상 (의미 고정)
| 색상 | 의미 | 예시 |
|---|---|---|
| Cyan `(60-90,180-220,255)` | 정보/현재값/탐색 | FOLDER/SORT 배지, BPM 값 테두리, 스크롤 썸 |
| Yellow `(255,210-230,50-90)` | 핵심 강조/현재 선택값 | 옵션 모달 선택된 값, BPM 숫자, 하이라이트 |
| Red `(240-255,50-100,50-100)` | 위험/실패 | FAILED 램프, Danger 게이지, 종료 확인 모달 |
| Green/Mint `(60-100,210-255,80-150)` | 성공/클리어 | CLEARED/FULL COMBO 램프 |
| 난이도 티어 컬러 (`level_color`) | Normal→Hyper→Another→Insane→Overjoy | LV 배지, 상세 패널 LEVEL 박스 — 반드시 `level_tier_label()`로 색 옆에 티어 이름을 같이 표기해 범례 역할을 하게 할 것 |

### 2.3 배경 명도 사다리 (테두리 대신 이것을 쓴다)
패널을 구분할 때 테두리를 새로 긋지 말고 아래 사다리에서 한 단계 밝은/어두운 값을 고릅니다.

| 단계 | RGB 범위 | 용도 |
|---|---|---|
| Base | `(8,8,12)` | 화면 최하단 배경 |
| Panel | `(14-17,16-21,26-33)` | 리스트/디테일 영역 같은 1차 컨테이너 |
| Card | `(20-24,24-28,35-42)` | 패널 안의 통계 박스, Personal Best 카드 |
| Row (alt) | 홀/짝 행 `±6~7` 명도 차이 | 리스트 행 줄무늬 (스트라이프), 보더 없이 구분 |
| Tinted Card | Card 값에 액센트 색을 `channel/6` 정도 더함 | 의미 있는 색(난이도 등)을 배경 자체에 녹여 범례처럼 쓸 때 |

---

## 3. 테두리(보더) 사용 정책

**원칙: 테두리는 "의미"가 있을 때만 그린다.** 패널을 나누는 용도로는 테두리 대신 2.3의 배경 명도 사다리를 쓴다.

### 테두리를 유지해도 되는 경우 (허용 목록)
- 선택/포커스 상태 표시 (선택된 곡 행, 선택된 옵션 행/메뉴 항목)
- 모달/다이얼로그 윤곽선 (배경과 분리되어 떠 있음을 알려야 함)
- 기능적 입력 필드 (검색창 — 활성/비활성 상태에 따라 색이 바뀌는 등 실질적 피드백이 있는 경우)
- 의미 있는 상태 표시자 (게이지 바 테두리, 위험 상태 점멸 테두리, 등급(Rank) 박스, FAST/SLOW 같은 판정 색상 박스)

### 테두리를 제거하고 배경 대비로 바꿔야 하는 경우
- 반복되는 리스트 행 하나하나를 감싸는 1px 박스 (150개 행 × 4변 = 순수 노이즈)
- 패널 안에 또 패널을 두는 중첩 박스 (상세 패널, 통계 박스, Personal Best 카드 등)
- 순수 장식용 프레임 (이미지 영역을 감싸는 1px 하이라이트 등, 이미지/배경 자체 명도차로 충분한 경우)

---

## 4. 리스트/모달 그룹핑 컨벤션

항목이 8개를 넘는 평면 리스트(옵션 모달 등)는 성격별로 묶고 그 경계에 **섹션 헤더**(Label 스케일 + 얇은 1px 구분선)를 넣습니다.

- 섹션 헤더는 순수 렌더링 레이어에서만 삽입합니다. 입력 핸들러(`handlers/*.rs`)의 인덱스 기반 로직은 건드리지 않고, 기존 배열 순서/인덱스를 그대로 유지한 채 "이 인덱스 앞에 헤더를 그린다"는 매핑만 추가합니다. (`modals.rs`의 `SECTION_HEADERS` 참고)
- 섹션 헤더 색상은 Cyan 계열(`(100,160,220)`)로 통일합니다.

---

---

## 5. 제안: 디자인 토큰 및 컴포넌트 명세 (Refresh)

아래는 디자인 리프레시를 위해 제안된 토큰 표다. 전체 화면에 채택되지 않았으며, `Viewport::scale`(`s`)를 곱하는 방식 등은 현재 구현의 `theme.rs`와 대조해 개별 변경에서 검토한다.

### 5.1 Color Tokens (Semantic Palette)

| 토큰 | Hex (sRGB) | 의미 / 용도 |
|---|---|---|
| `surface_base` | `#08080C` | 화면 전체 배경 |
| `surface_panel` | `#0E101A` | 1차 컨테이너 (리스트 영역, 상세 패널) |
| `surface_card` | `#141823` | 패널 내 통계 박스, Personal Best 카드 |
| `surface_card_tinted` | 동적 | 액센트 색 `channel/6` 혼합 (난이도 등 범례용) |
| `surface_row_alt` | `panel ±0x07` | 홀/짝 행 줄무늬 (보더 없이 구분) |
| `surface_overlay` | `#101420CC` | 모달 오버레이 (알파 80%) |
| `border_subtle` | `#23283C` | 모달/카드 외곽, 구분선 |
| `border_focus` | `#50BCFF` | 선택/포커스/입력 활성 상태 |
| `border_danger` | `#F03A3A` | 위험/실패 상태 테두리 |
| `text_primary` | `#FFFFFF` | 선택된 행, 모달 제목, 핵심 값 |
| `text_secondary` | `#BEC3D7` | 비선택 곡 제목, 일반 본문 |
| `text_tertiary` | `#7882A0` | 박스 캡션(`BPM`, `NOTES`), 섹션 헤더 |
| `text_hint` | `#646E87` | 하단 단축키 힌트, 각주 |
| `accent_cyan` | `#50BCFF` | 정보/현재값/탐색 (FOLDER/SORT 배지, 스크롤 썸) |
| `accent_yellow` | `#FFE632` | 핵심 강조/선택값 (옵션 선택값, BPM 숫자) |
| `accent_red` | `#F04646` | 위험/실패 (FAILED 램프, 종료 확인) |
| `accent_green` | `#3CDC96` | 성공/클리어 (CLEARED/FULL COMBO) |
| `accent_orange` | `#FF8C32` | SLOW 판정, 경고 |
| `accent_mint` | `#50FFB8` | PERFECT GREAT, 양호 상태 |
| `difficulty_normal` | `#60E060` | Normal 티어 |
| `difficulty_hyper` | `#50B0FF` | Hyper 티어 |
| `difficulty_another` | `#E050FF` | Another 티어 |
| `difficulty_insane` | `#FF5050` | Insane 티어 |
| `difficulty_overjoy` | `#FFD030` | Overjoy 티어 |

> **난이도 티어 컬러**는 반드시 `level_tier_label()`로 색 옆에 티어 이름(예: `NORMAL`, `HYPER`)을 같이 표기해 범례 역할을 하게 합니다.

### 5.2 Spacing Scale (s-based)

| 토큰 | 공식 | 용도 |
|---|---|---|
| `space_xs` | `4 * s` | 배지 패딩, 내부 여백 최소 |
| `space_sm` | `8 * s` | 카드 내부 패딩, 요소 간 간격 |
| `space_md` | `16 * s` | 섹션 간 간격, 카드 마진 |
| `space_lg` | `24 * s` | 대형 섹션 간격 |
| `space_xl` | `32 * s` | 화면 가장자리 마진 |

> `s = Viewport::scale` (720p 기준 1.0). 모든 픽셀 값은 토큰 × s 로 계산.

### 5.3 Typography Scale (4-tier + Bold variants)

| 티어 | 스케일 공식 | 글리프 크기 | 용도 | 헬퍼 |
|---|---|---|---|---|
| **Title** | `(2.0 * s).round().max(1.0)` | 16x14 (ASCII), 20x16 (CJK) | 화면/모달 제목, 선택된 곡 제목 | `draw_text_with_shadow` |
| **Value** | `(1.1 * s).round().max(1.0)` | 16x14 / 20x16 | 핵심 수치 (EX Score, BPM, 콤보) | `draw_bold_text` |
| **Body** | `(0.9 * s).round().max(1.0)` | 16x14 / 20x16 | 본문, 라벨 값 | `draw_text` |
| **Label** | `(0.7 * s).round().max(1.0)` | 12x10 / 16x12 | 박스 캡션, 섹션 헤더 | `draw_text` |
| **Hint** | `Body` 명도만 낮춤 | 동일 | 하단 단축키, 각주 | `draw_text` |

> **비트맵 확장**: ASCII 5x7 → **16x14**, Hangul/Kana 10x8 → **20x16**, Bold 8x12 유지. 스케일 적용 시 `scale * 2`(Title), `scale * 1.1`(Value), `scale * 0.9`(Body) 배율로 렌더링.

### 5.4 Shadow / Glow

| 토큰 | 값 | 용도 |
|---|---|---|
| `shadow_drop` | `offset(1*s,1*s) alpha=60% black` | Title 텍스트, 선택 카드 |
| `shadow_card` | `box-shadow 0 2*s 8*s rgba(0,0,0,0.4)` | 모달, 팝오버 |
| `glow_focus` | `inset border 2*s accent_cyan` | 선택된 리스트 행, 입력 필드 활성 |
| `glow_rank_MAX` | `border 2*s #FFD700` | MAX 랭크 휘장 |
| `glow_rank_AAA` | `border 2*s #FFDC32` | AAA 랭크 휘장 |

### 5.5 Border Radius (Corner Radius)

| 토큰 | 값 | 용도 |
|---|---|---|
| `radius_none` | `0` | 리스트 행, 풀폭 바 |
| `radius_sm` | `4 * s` | 배지, 버튼, 입력 필드 |
| `radius_md` | `8 * s` | 카드, 패널, 모달 |
| `radius_lg` | `12 * s` | 대형 컨테이너 |

> tiny-skia 는 라운드 렉트 네이티브 지원 안 함 → `draw_rounded_rect` 헬퍼로 근사 (4코너 각각 1px 단위 채움). 성능 위해 `radius_sm`까지만 라운드, `radius_md` 이상은 직각 + 글로우로 대체.

### 5.6 제안 컴포넌트 명세

아래는 제안된 API의 예시이며, 현재 `SoftwareRenderer` 구현 지시가 아니다. 실제 컴포넌트와 렌더 API는 `beetle-render` 구현을 확인한다.

- **Panel**: `render_panel(rect, variant)` — `variant`: `Base`/`Card`/`Overlay`.
- **Card**: `render_card(rect, title, children)` — 배경 `surface_card`, 상단 `border_subtle`.
- **Button**: `render_button(rect, label, state)` — `state`: `Default`/`Hover`/`Selected`/`Disabled`, `radius_sm`.
- **Badge**: `render_badge(rect, label, value, color)` — 좌측 Label 라벨 + 우측 Bold Value.
- **ProgressBar**: `render_progress_bar(rect, progress, color, show_text)` — 게이지 바, `radius_sm`.
- **Modal**: `render_modal(rect, title, sections)` — 오버레이 + 컨테이너 + 제목 + 섹션 헤더.
- **HeaderBar**: `render_header_bar(title, badges[], search_active, query)` — 상단 바 + 검색창.

### 5.7 화면별 적용 가이드

| 화면 | 메인 컴포넌트 | 특이 사항 |
|---|---|---|
| **SongSelect** | `HeaderBar`, `Panel(Base)` 리스트, `Panel(Card)` 디테일, `Card` PB | 리스트 행: `surface_row_alt` 홀짝, 선택 행 `glow_focus` |
| **Gameplay** | `Panel(Card)` HUD, `ProgressBar` 게이지, 판정 팝업 | 판정 팝업: `glow_rank_*` + `Value` bold |
| **Result** | `Modal` 랭크 휘장, `Card` 3컬럼, `ProgressBar` 히스토그램 | 랭크 휘장 `glow_rank_*`, 이전 기록 비교 `accent_green` |
| **KeyConfig** | `Panel(Card)` 테이블, `Button` 리바인드 | 선택 레인 `glow_focus`, 양쪽 컬럼 `Panel(Card)` |
| **Loading** | `Panel(Overlay)` 아트워크, `ProgressBar` 로딩 | 아트워크 프레임 `border_subtle` 유지 |

### 5.8 금지/허용 규칙 (Guardrails)

**허용**: 모든 색상은 `Color Tokens` 표에서만, 간격은 `Spacing Scale` 표에서만, 텍스트 크기는 `Typography Scale` 4티어 중 하나, 테두리는 의미 있는 상태(선택/포커스/위험/게이지)일 때, 라운드는 `radius_sm`/`radius_md`만.

**금지**: `ColorRgba::new(...)` 직접 호출, 매직 넘버 픽셀 값, 임의 스케일 값, 장식용 1px 박스(배경 명도 대비로 대체).

---

## 6. 제안 토큰과 기존 원칙의 관계

섹션 2.3(배경 명도 사다리), 3(테두리 사용 정책), 4(리스트/모달 그룹핑 컨벤션)의 시각적 의도는 아래 제안 토큰에 대응한다. 이는 구현된 토큰 이름이나 현재 모든 화면의 사용을 보장하지 않는다.

| 기존 규칙 | 새 토큰 매핑 |
|---|---|
| 2.3 배경 명도 사다리 | `surface_*`, `surface_row_alt` |
| 3. 테두리 허용 목록 | `border_focus`, `border_danger`, `accent_*` |
| 3. 테두리 제거 → 배경 대비 | `surface_card_tinted`, `surface_row_alt` |
| 4. 섹션 헤더 | 5.7 `Modal` 섹션 헤더 (Label + accent_cyan) |

---

## 7. 리프레시 제안의 당시 체크리스트

- [ ] `docs/proposals/ui-design-system-refresh.md` — 제안 당시의 결정 로그 작업
- [ ] 디자인 토큰 모듈 신설 (`beetle-render/src/design_tokens.rs`)
- [ ] 컴포넌트 라이브러리 신설 (`beetle-render/src/components/`)
- [ ] 비트맵 폰트 엔진 확장 (16x14 ASCII, 20x16 CJK, Bold variants)
- [ ] GDI 폴백 폰트 크기/품질 상향 (-10 → -18px, MS YaHei UI)
- [ ] SongSelect 리디자인 (HeaderBar, Panel, Card, Badge 적용)
- [ ] Result 리디자인 (Modal, Card, ProgressBar, Glow)
- [ ] KeyConfig 리디자인 (Panel/Card 테이블, Button 리바인드)
- [ ] Gameplay HUD 리디자인 (ProgressBar 게이지, 판정 팝업 Glow)
- [ ] Loading/Options/Exit 모달 리디자인 (Modal 컴포넌트)

> 이 체크리스트는 활성 작업 목록이 아니다. 현재 적용 여부를 확인해야 할 때만 각 구현과 대조한다.

---

## 8. 제안의 비고 (현재 플랫폼 지침 아님)

- **Linux CJK**: GDI 없음 → square-glyph fallback(기존) 유지. 향후 필요시 미리 렌더링된 한자 아틀라스(`.bmp` + 메타데이터) 로드 방식으로 확장 가능(런타임 힙 할당 없음, AGENTS.md §2 INV-3 준수).
- **애니메이션**: 프레임 단위 인터폴레이션(`lerp`)만 사용, 외부 트윈 라이브러리 금지.
- **성능**: 토큰/컴포넌트는 `const fn`/`inline`으로 제로코스트 추상화 유지.

새로 화면을 고칠 때마다 이 체크리스트를 갱신하세요.
