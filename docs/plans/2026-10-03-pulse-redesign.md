# PULSE 전면 리디자인 (2026-10-03)

## 배경

이전 세션(같은 날 이전 작업, `2026-10-03-gameplay-hud-redesign.md`)에서는
화면 하나씩 기존 톤(파랑/노랑/초록 등 6색 조합, 박스 테두리 위주)을 유지한 채
컴포넌트만 교체하는 점진적 패치를 했다. 사용자가 이 결과에 "여전히 잘
모르겠다"는 피드백을 주면서, 점진적 패치가 아니라 **처음부터 새로
상상한 디자인**을 원한다고 명확히 했다. 그래서 전체 화면(SongSelect /
Gameplay / Result)을 하나의 일관된 디자인 언어로 먼저 HTML 목업으로
설계하고, 사용자 승인을 받은 뒤 실제 Rust 엔진에 화면 단위로 포팅하는
방식으로 전환했다.

## 승인된 방향: PULSE

목업: `sketches/pulse-redesign/index.html` (브라우저로 직접 열어서 3개
탭으로 SongSelect/Gameplay/Result 비교 가능). 디스포저블 스케치지만,
이 라운드의 유일한 참조 디자인이므로 리디자인이 끝날 때까지는 저장소에
남겨둔다.

핵심 원칙:
1. **단일 액센트 정체성**: 기존처럼 UI 크롬(패널, 버튼, 강조선)에 6가지
   색을 섞어 쓰지 않고, 시안↔마젠타 듀오톤(`ColorToken::PULSE_CYAN` /
   `PULSE_MAGENTA`, design_tokens.rs) 하나로 통일한다. 판정 등급 색
   (PGREAT 골드, GREAT 오렌지 등 `ACCENT_*`/`DIFF_*`)은 그대로 유지하되
   오직 판정 데이터 표현에만 쓰고 UI 크롬에는 섞지 않는다 — 두 색 어휘가
   절대 충돌하지 않게.
2. **박스 테두리 축소**: 모든 패널/행/카드에 테두리 박스를 두르던 기존
   방식 대신, 헤어라인 1px 구분선 + 배경 명도 단계로 구획을 나눈다.
   테두리는 선택/포커스 표시, 모달 외곽선, 위험 상태처럼 "의미 있는"
   경우에만 남긴다 (native-app-visual-review 스킬의 border-usage
   원칙과 일치).
3. **대각선 컷 크롬**: PLAY 버튼, 결과 화면 랭크 배지 등 핵심 CTA는
   직사각형 대신 모서리 하나를 대각선으로 자른 사변형
   (`draw_cut_quad`)으로 그려 "아케이드 캐비닛" 느낌을 준다.
4. **세그먼트 judge bar**: 판정 내역(PGREAT/GREAT/GOOD/BAD/POOR/MISS)을
   텍스트 리스트나 그리드 대신 하나의 가로 바를 등급별 색으로 비례
   분할해서 보여준다 (IIDX/SDVX류 상업 타이틀의 실제 관례).
5. **과감한 타이포그래피**: 큰 볼드 숫자(점수, 콤보, 랭크) + 작은
   캡션형 마이크로 레이블(색: TEXT_TERTIARY 상당)의 2단 구성을 기본
   패턴으로 쓴다.

## 엔진에 추가된 PULSE 전용 프리미티브

`crates/beetle-render/src/components/mod.rs`에 tiny-skia의 네이티브
Path/Shader 파이프라인(이미 허용된 핵심 크레이트, AGENTS.md 의존성
정책 위반 아님)을 사용하는 3개 함수를 추가했다. 기존 `draw_rect`는
직접 픽셀 버퍼를 때리는 고속 경로라 그라디언트/비축정렬 도형을 못
그리므로, 이 셋만 tiny-skia의 `fill_path`/`fill_rect` + `Paint`/
`LinearGradient`를 쓴다. 게임플레이 핫패스(노트/레인 렌더링)는
건드리지 않고 그대로 `draw_rect` 고속 경로를 쓴다.

- `draw_gradient_rect(x,y,w,h,color_start,color_end,horizontal)` —
  2-스탑 선형 그라디언트 사각형 (선택 행 페이드 워시, 재킷 하단 스크림 등)
- `draw_cut_quad(x,y,w,h,cut,color_start,color_end)` — 왼쪽 위 모서리를
  `cut` 픽셀만큼 대각선으로 자른 사변형, 그라디언트 채움 (PLAY CTA 등)
- `draw_corner_triangle(x,y,size,corner,color)` — 박스 한쪽 모서리에
  직각삼각형 액센트 (재킷 코너 슬래시)

## 화면별 체크리스트

- [x] SongSelect — PULSE로 전면 재작성 완료 (이번 세션)
  - [x] 헤더: BEETLE 워드마크 2-톤(흰/시안) + 언더라인 탭 (박스 배지 제거)
  - [x] 검색창: 테두리 박스 제거, 활성 시 시안으로 빛나는 헤어라인만
  - [x] 곡 리스트: 행별 박스 배경 제거, 헤어라인 구분선, 선택 행은
        시안→마젠타 그라디언트 엣지 바 + 좌측 페이드 워시
  - [x] 상세 패널: 재킷 코너 트라이앵글 액센트 + 하단 그라디언트 스크림,
        스탯은 박스 그리드 → 레이블/값 2단 컬럼, Personal Best 박스 제거
        (헤어라인 + 우측 정렬 값), PLAY 버튼 → 대각선 컷 그라디언트 CTA
  - [x] `cargo test -p beetle-render --release` 41/41 통과,
        `computer_use`로 실제 렌더 확인 (목업과 구조적으로 일치)
- [ ] Gameplay — PULSE 적용 (주의: 소프트웨어 경로 `gameplay.rs` +
      GPU 경로 `gameplay_gpu.rs` 둘 다 고쳐야 함, 상세는
      `2026-10-03-gameplay-hud-redesign.md` 참고. 세그먼트 judge bar로
      교체, 콤보/스코어 패널에 대각선 컷 적용 검토)
- [ ] Result — PULSE 적용 (랭크 배지 대각선 컷 + 글로우, 세그먼트
      judge bar, 그래프 영역)
- [ ] KeyConfig
- [ ] Modals

## 알려진 사소한 렌더링 이슈 (SongSelect)

- Personal Best "ACCURACY" 행의 `0.00% [F]` 문자열에서 `%`와 `[F]`
  사이 비트맵 볼드 폰트 커닝이 살짝 벌어져 보임 — 폰트 자체의 커닝
  특성이고 레이아웃 버그 아님, 추후 폰트 작업 때 같이 볼 것.

## 검증

- `cargo build --release -p beetle-app` 성공
- `cargo test -p beetle-render --release`: 41/41 통과
- `computer_use`로 실제 SongSelect 화면 캡처 확인 — 목업
  (`sketches/pulse-redesign/index.html`)과 구조/색/레이아웃 일치
