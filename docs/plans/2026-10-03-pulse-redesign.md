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
- [x] Gameplay — PULSE 적용 완료 (이번 세션)
  - [x] 소프트웨어 경로(`gameplay.rs::draw_hud_info`): `draw_cut_quad`
        대각선 컷 그라디언트 스코어 클러스터(EX SCORE/ACCURACY/PACEMAKER),
        세그먼트 judge bar + 2열 범례(색 점+라벨+카운트), 재킷과 같은
        `draw_corner_triangle` 코너 액센트로 SongSelect와 시각적 연속성 확보
  - [x] GPU 경로(`gameplay_gpu.rs`): `SpriteBatcher`는 path-fill/그라디언트
        미지원이라 대각선 컷/그라디언트는 포팅 불가 — 세그먼트 judge bar만
        단색 사각형으로 구현, 기존 "모든 rect 먼저 → 모든 text 나중" 패스
        구조를 지키기 위해 바를 BGA 프레임 rect 다음(텍스트 패스 시작 전)에
        끼워 넣음. 2열 텍스트 그리드는 바 밑 범례로 용도 변경
  - [x] `cargo test -p beetle-render --release` 41/41 통과
        (`test_render_gameplay_gpu_batched_draw_calls` 포함),
        `computer_use`로 GPU 경로 실제 렌더 확인 (이 머신의 활성 경로)
  - [ ] 소프트웨어 경로는 빌드/테스트만 확인, 이 머신은 GPU 경로가
        활성이라 실제 화면으로는 아직 못 봄 — D3D11 비활성 환경에서
        확인 필요
- [x] Result — PULSE 적용 완료, 실제 화면 확인 완료 (이번 세션)
      (HAZARD 게이지로 전환해 미스 1회로 즉시 스테이지 실패 → Result
      진입시켜 빠르게 검증. `computer_use` 캡처로 확인: 헤어라인 분리,
      세그먼트 바, F 랭크 글로우, 코너 액센트 전부 겹침/깨짐 없이 정상
      렌더링됨)
  - [x] 헤더: "STAGE RESULT" 뱃지 박스 → 레이블 텍스트 + 시안 그라디언트
        언더라인 (SongSelect/Gameplay와 동일 패턴)
  - [x] 랭크 에블럼: 박스 테두리 제거, 큰 볼드 텍스트 + 같은 텍스트를
        낮은 알파로 한 번 더 깔아 글로우 흉내, 배경은 랭크색 세로
        그라디언트 워시. NEW RECORD는 대각선 컷 마젠타 배지
  - [x] 중앙 컬럼: EX SCORE/ACCURACY/MAX COMBO를 박스 패널 → 헤어라인
        구분 레이블/값 열로, JUDGE BREAKDOWN은 Gameplay와 동일한
        세그먼트 바 + 2열 범례로 교체 (박스 리스트 제거)
  - [x] FAST/SLOW: 박스 2개 → 헤어라인 구분 레이블/값 쌍
  - [x] 타이밍 오프셋 히스토그램: 박스 테두리 제거(하단 헤어라인만),
        중심선 시안, FAST 쪽 막대 시안, SLOW 쪽 막대 마젠타로 듀오톤 통일
  - [x] `cargo build --release -p beetle-app` 성공,
        `cargo test -p beetle-render --release` 41/41 통과
  - [x] **TIP**: Result는 끝까지 플레이해야 도달하는데, 곡 길이만큼
        기다리는 대신 GAUGE를 HAZARD로 바꾸고(F6) 아무 키도 안 눌러서
        미스 1회로 즉시 스테이지 실패시키면 몇 초 안에 Result 화면에
        도달한다 — 다음 화면(KeyConfig는 해당 없음) 검증 때도 쓸 수 있는
        방법.
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

## 폰트 시스템 교체: 손그림 비트맵 → 임베디드 TrueType (fontdue)

세션 후반에 사용자가 반복적으로 폰트 품질을 지적("폰트 렌더링이 진짜
완전 엉망이네", "상업 게임 수준 너무 낮게 보고 있는거 아니야?"). 이
라운드 초반에 한 synthetic-bold ASCII 딜레이션, GDI 한자 폴백 스케일
수정은 실제 개선이었지만 여전히 "서로 다른 두 손그림 그리드를 억지로
맞추는" 땜질이었다는 게 최종 결론.

### 검토한 대안과 탈락 이유
1. **GDI 폴백을 ASCII까지 확장** — 바이너리 크기 0 증가, 기존 인프라
   재사용이라 유리해 보였지만 실제로 깨짐: MS Gothic의 "반각" 라틴
   글립이 CJK 그리드 기준 폭이라 우리 좁은 ASCII 칸(5유닛)에 안 맞고
   옆 글자와 겹침. 시도 후 되돌림.
2. **런타임 웹폰트 다운로드** — AGENTS.md "독립 실행" 원칙(외부 런타임
   의존 없는 단일 정적 바이너리) 정면 위배로 기각.
3. **채택: fontdue(순수 Rust, GPU 비의존) + 임베디드 서브셋 폰트 3개**

### 최종 구성
- JetBrains Mono (OFL-1.1) Basic Latin만, Regular+Bold: 53KB × 2
- Noto Sans JP (OFL-1.1) 조요칸지 2137자(MEXT 공식 리스트) + 가나 +
  CJK 구두점: 982KB
- Noto Sans KR (OFL-1.1) KS X 1001 상용 한글 2,350자(완성형 11,172자
  전체 대신 — 2.76MB → 496KB로 축소) + 자모: 496KB
- 폰트 에셋 합계 ~1.55MB, 최종 `beetle-app.exe` ~3.15MB.
  **AGENTS.md의 "<1MB" 목표를 의도적으로 초과한 결정** — §3 금지
  목록에 fontdue 예외 조항으로 명시.

### 구현 (`bitmap_font/truetype_font.rs`)
- 글립마다 48px EM으로 래스터한 뒤, 기존 손그림 비트맵이 쓰던 것과
  동일한 고정폭 셀(ASCII 5x7, CJK/한글/가나 10x8)에 맞춰 리샘플 —
  화면별로 손으로 맞춰둔 레이아웃/커닝 계산을 하나도 건드리지 않음.
- **높이 기준 리사이즈 → 그래도 폭 초과시 폭 기준으로 재조정**: GDI를
  ASCII까지 확장했을 때 겹침 버그가 난 이유(폭 제약 없이 높이만
  맞춤)를 구조적으로 막음.
- 서브셋 밖 문자(희귀 한자, 특수 기호)는 기존 손그림 테이블 /
  Windows GDI 폴백으로 자동 폴백.

### 검증 중 발견한 버그
첫 빌드에서 텍스트가 거의 안 보일 정도로 흐릿하게 나옴 — 원인은
최근접 샘플링(nearest-neighbor) 다운샘플링: 48px 래스터를 5~10px
셀로 줄이는 5~8배 축소 비율에서 점 샘플링은 글립의 실제 잉크 픽셀
대부분을 건너뛰고 흐릿한 안티에일리어싱 가장자리만 주움. 박스
필터(영역 평균) 다운샘플링으로 교체해서 해결.

### 검증
- `cargo test --workspace --release`: 155/155 통과 (한 벤치마크성
  테스트는 전체 병렬 실행시 타이밍에 민감해서 플레이키 — 단독/4스레드
  실행시 통과 확인, 폰트 변경과 무관)
- `computer_use`로 SongSelect 실 화면 확인: 라틴/한자/가나 혼합 제목
  ("星の器〜STAR OF ANDROMEDA(N...")) 두께 일관, 겹침/잘림 없음,
  목록 행의 작은 크기에서도 가독 가능
- 커밋 `f780de1`
