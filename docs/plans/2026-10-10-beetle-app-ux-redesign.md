# beetle-app UI/UX 개편 계획 (2026-10-10)

대상: `crates/beetle-app`, `crates/beetle-render/src/screens/`

## 배경

`2026-10-04-d3d11-ui-rebuild.md`(P0~P4)에서 렌더러·텍스트·스킨·다섯 화면의 **겉모습**은
상용 수준 기반으로 다시 만들었다. 이번 개편은 그 위에서 **쓰는 흐름(UX)**을 고친다.
화면을 새로 그리는 일보다 입력 모델, 정보 구조, 화면 간 흐름을 바로잡는 일이 중심이다.

## 진단 (2026-10-10 코드·캡처 기준)

### 전역
1. **키보드 전용**: 마우스·휠 입력을 전혀 처리하지 않는다(`main.rs`의 `WindowEvent`에
   `CursorMoved`/`MouseInput`/`MouseWheel` 없음). 컨트롤러(건반) 키로 메뉴를 조작하는
   경로도 없다 — 메뉴는 화살표·Enter 고정.
2. **IME 없음**: 검색은 `KeyEvent::text`만 받는다. 한글·일본어 곡명을 입력해서 찾을 수 없다.
3. **언어 불일치**: 게임은 영어 UI, `bpm-gui`는 한국어 UI(2026-10-10 개편). 문자열이
   화면 코드에 흩어져 있어 한쪽으로 맞추기도 어렵다.
4. **화면 전환 연출 없음**: 화면이 컷으로 바뀐다(`motion.rs`의 이징은 화면 내부 연출에만 쓰임).
5. **숨은 기능**: `.bmsp` 끌어다 놓기(`main.rs` `DroppedFile`), F5 재스캔, `C`=키설정,
   `O`=옵션 같은 단축키가 화면 어디에도 없거나 푸터 키캡 9개 사이에 묻혀 있다.

### 선곡 (SongSelect)
6. **폴더가 평면 순환**: F1/F3로 `All → 5K → 7K → 9K → 10K → 14K → Level → ClearStatus →
   난이도표…`를 한 줄로 돈다(`state::SongCategory`). 원하는 폴더까지 여러 번 눌러야 하고,
   난이도표가 늘수록 더 길어진다. 레벨별·램프별로 들어가 고르는 계층이 없다.
7. **같은 곡의 채보가 따로 줄 선다**: `AIRSHAVER [7key, Another]`/`[14key, Another]`처럼
   난이도마다 행이 하나씩이라 목록이 길고, 같은 곡의 다른 난이도로 옮겨 가는 동작이 없다.
8. **정렬은 F2 순환뿐**, 필터(모드·레벨 범위·미플레이만 등) 없음.
9. **빈 서재 안내 부족**: 곡이 하나도 없을 때도 "No songs in this folder"만 나온다.
   곡을 넣는 방법(bpm-gui, 끌어다 놓기, songs 폴더)을 알려 주지 않는다.

### 플레이 옵션 모달
10. **플레이 옵션과 시스템 설정이 한 모달**: HI-SPEED·GAUGE처럼 곡마다 바꾸는 값과
    RESOLUTION·GRAPHICS(재시작 필요)·TARGET FPS처럼 한 번 정하는 값이 16행에 섞여 있다.
11. **행 번호 하드코딩**: `handlers/options.rs`가 `0 => … 15 =>`로 행을 분기한다. 행 하나를
    추가·재배치하면 처리기와 그리기 코드를 같이 고쳐야 한다.
12. **설명 없음**: MODIFIER/GAUGE/LN MODE 값이 무엇을 뜻하는지 화면에 없다.
13. **HI-SPEED가 px/s**: 해상도에 따라 체감이 바뀌고, BMS 플레이어 관례(그린 넘버 =
    노트가 보이는 시간)와 다르다. 판정 오프셋도 측정 도구 없이 숫자만 맞춘다.

### 게임플레이
14. **BGA가 없으면 오른쪽 절반이 빈 상자**("NO BGA")로 남는다.
15. **플레이 중 키 안내 상주**(`KEYS Shift+S D F …`) — 첫 플레이 이후엔 소음이다.
16. **시작·종료 연출 없음**: 곡 시작 전 준비 구간, FULL COMBO/CLEAR/FAILED 연출이 없다.
17. **하이스피드 변경 피드백**이 약하다(바꾼 값·그린 넘버를 잠깐 띄워 주는 표시 없음).

### 결과 · 키 설정 · 로딩
18. 결과: 게이지 추이 그래프 없음. 다음 행동이 Enter/R/P 세 개뿐(옵션 바꿔 재도전 없음).
19. 키 설정·로딩은 구조가 괜찮다 — 언어·입력 모델 정리만 따라간다.

## 원칙

- AGENTS.md 불변식 유지: 판정·노트 위치는 오디오 클럭(INV-1), 무거운 일은 워커(INV-5).
- **새 크레이트 없음**. 마우스·IME는 `winit` 이벤트, 문자열 표는 직접 작성.
- 색은 `theme.rs` 토큰만, 좌표는 간격 스케일과 `Rect` 분할 헬퍼만(P4 규칙 그대로).
- 화면 전체 1 드로우콜(외부 텍스처 +1) 유지, WARP 60 fps 유지(`Ui::lite`).
- 화면마다 커밋 + 실제 앱 전/후 캡처 + 이 문서 갱신.

## 단계

### U0 — 방향 결정 + 기준 캡처
- [x] 결정 1~4 확정 (아래 **결정**)
- [x] 실제 앱(격리 작업 폴더)에서 화면별 "전" 캡처 일괄 생성, `tests/d3d11_*.rs` 캡처와 대조
  - **기준 캡처 결과 (2026-10-10)**
  - 위치: `scratch/ux-before/raw/`(실제 앱 1280x720 창 모드, PNG), `scratch/ux-before/tests/`(`target/`의 테스트 캡처 복사본). `scratch/`는 커밋하지 않는다. 실행 폴더는 `scratch/ux-before/run/`(곡·테이블은 정션, `config.dat`는 복사본).
  - 캡처한 화면: boot, songselect, songselect + 옵션 모달(`select-options`), + 종료 모달(`select-exit`), loading, gameplay 7K 자동 플레이(`play-7k`, NO BGA 상자 포함), gameplay 7K 비자동(`play-7k-manual`), gameplay 8레인 7K 곡(`play-7k-nijinowasure`), result(자동 플레이), keyconfig 6K·7K(`keyconfig`, `keyconfig-7k`).
  - 생략: 빈 서재 안내(곡 216개가 있어 빈 상태를 만들지 않음, 진단 9 미확인). AAA 클리어 결과는 자동 플레이 결과(`SCORE NOT SAVED`)로 대신하고 테스트 `result-clear`와 구조만 비교.
  - FPS(release, Direct3D 11 하드웨어): 부팅·선곡·게임플레이·결과 모두 60~61 fps. 선곡 폴더 전환 3 s 구간에서 56 fps 1회. 로딩 0.0 s 로그(`1 frames`)는 측정값이 아님.
  - 테스트 캡처와 차이(UI 로직 차이는 없음):
    - 데이터: 실제 서재 216곡, 테스트 픽스처 15곡. 제목·표지·점수가 다르다.
    - 플레이필드 위치: 실제는 `config.dat`의 `field_position=CENTER`(가운데), 테스트는 왼쪽 플레이필드와 오른쪽 상단 HUD. 설정 차이로 보이나 확인하지 않음.
    - 비자동 플레이 키 안내: 실제도 `KEYS Custom layout 1/2 SPEED F10/F11 COVER ESC PAUSE`가 상주(테스트와 같음). 자동 플레이에서는 `ESC Return to song select`만 나온다.
    - 로딩: 0.0 s 첫 프레임에 곡 카드(표지·제목·BPM·노트 수)가 없는 것은 버그가 아니다. 카드는 등장 애니메이션(슬라이드·페이드)으로 들어오므로 첫 프레임에는 진행 막대와 "Decoding keysounds and preparing audio"만 보인다. 테스트 캡처(0.1 s)에 카드가 있는 것도 같은 이유다.
    - 7K 표기 곡 `Beetle Demo Track`은 플레이필드가 6레인(스크래치 + 5키)으로 그려졌다. 같은 7K 표기의 `虹のわすれもの`는 8레인(스크래치 + 7키)으로 정상. 곡 데이터 특성으로 보이나 확인하지 않음.
  - 진단 항목 확인(화면에서 보이는 것):
    - 5 단축키가 푸터 키캡 9개에 묻혀 있음(선곡 하단): 확인.
    - 6 폴더가 `< ALL SONGS >` 한 줄 순환: 확인.
    - 7 같은 곡 채보가 행마다 따로 줄 섬(`ADDicTiON 4500000` 6UE21/6UE22, `DEATH†TENGOQ` 3행): 확인.
    - 8 정렬 `TITLE` 한 종류, 필터 칩 없음: 확인.
    - 10 플레이 옵션 모달에 PLAY / DISPLAY·SYSTEM / INPUT·SESSION / LAYOUT이 한 모달에 섞임: 확인.
    - 12 옵션 행 설명 없음: 확인.
    - 13 HI-SPEED가 `1125 px/s`: 확인.
    - 14 BGA 없음 시 "NO BGA" 빈 상자: 확인.
    - 15 비자동 플레이에서 키 안내 상주: 확인(자동 플레이에서는 없음).
    - 18 결과에 게이지 추이 그래프 없음, 다음 행동은 ENTER/R/P: 확인.
    - 19 키 설정 구조는 정상. U0.5 일자 배치가 실제 앱 6K에 반영됨(`Keys in one straight row`): 확인.
    - 판단 불가(정적 캡처 범위 밖): 1~4(입력·IME·언어), 9(빈 서재), 11(행 번호), 16(시작·종료 연출: 게임 시작 후 4 s 시점에는 준비 표시 없음, 구간 길이 미확인), 17(하이스피드 변경 표시).

### U0.5 — 키 설정: 4K/6K 일자 배치
게임플레이의 4K/6K는 일자 레인인데, 키 설정 화면은 7K처럼 위/아래 두 줄 지그재그로 그린다
(`target/keys-6k.bmp`: Z·X·C·V 아래, S·F 위). 일자 배치는 지금 8K(트리거 형태가 아닐 때)에만
적용된다.
- [x] `crates/beetle-render/src/screens/key_bind.rs`의 `controllers()`에서
      `let straight = f.mode == PlayMode::Keys8 && !triggers;`를 4K·6K도 포함하도록 변경:
      `matches!(f.mode, PlayMode::Keys4 | PlayMode::Keys6) || (f.mode == PlayMode::Keys8 && !triggers)`.
      `side_width()`는 이미 `straight`를 받으므로 같은 값이 전달되면 폭 계산도 맞는다.
- [x] 4K·6K에 스크래치 레인이 없는지 확인(`SkinConfig::set_play_mode`의 레인 목록). 있다면
      일자 배치에서 스크래치를 어떻게 둘지 먼저 묻는다.
- [x] `layout_summary()`의 4K·6K 안내 문구를 일자 배치에 맞게(예: "Keys in one straight row").
- [x] 테스트: 같은 파일의 `every_mode_is_one_batch`, `sides_fit_the_screen` 통과 확인.
      `crates/beetle-render/tests/d3d11_keys.rs`를 돌려 `target/keys-4k.bmp`, `target/keys-6k.bmp`가
      한 줄로 나오는지 눈으로 확인(`cargo test -p beetle-render --release --test d3d11_keys`).
- [x] `cargo fmt --all` 후 커밋: `fix(render): 4K/6K key config draws keys in one straight row`.

### U1 — 입력 기반 (마우스 · 메뉴 액션 · IME · 문자열 · 전환)
U1은 다섯 단계로 나눈다. 화면마다 커밋하고 실제 앱 전/후 캡처를 남긴다.

- **U1a — 마우스 기반 + 선곡·모달 마우스** (완료)
  - [x] **클릭 영역**: 그리는 쪽이 `Hit` 목록을 기록하고(`crates/beetle-render/src/hit.rs`의 `HitId`,
        `Ui::hits`), 클릭·호버는 마지막으로 그린 프레임의 목록에서 찾는다(위에 그린 것 우선).
        레이아웃 계산을 처리기에 중복하지 않는다. 휠 = 선곡 목록 한 칸씩(옵션 모달에서는 행 이동).
  - [x] **선곡 화면**: 행 클릭 = 선택, 선택된 행 다시 클릭 = 재생. 폴더 `<` `>`, 정렬, 검색창,
        PLAY·REPLAY, 상단바 OPTIONS 버튼(결정 5). 옵션 모달: 행 선택, `<` `>` 값 변경, 바깥 클릭 = 닫기.
        종료 모달: 버튼, 바깥 클릭 = 취소. 게임플레이·로딩은 마우스를 무시한다(`handlers/mouse.rs`).
  - [x] **키와 같은 동작**: 키보드 처리기 본문을 이름 있는 함수로 빼서 키와 마우스가 함께 쓴다
        (`move_selection`, `cycle_folder`, `cycle_sort`, `start_selected`, `start_replay` in
        `handlers/song_select.rs`; `open_options`, `close_options`, `move_option_row`, `change_option`
        in `handlers/options.rs`). 키 동작은 그대로다.
  - 결과 (2026-10-10):
    - 크기: `beetle-app.exe` 3,012,096 B → 3,015,680 B (+3.5 KB, +0.12%).
    - 검증: `cargo test --workspace` 전부 통과. 새 테스트: `hit_at` 최상단 규칙, 선곡 화면의 히트 영역이
      그린 레이아웃과 맞고 모달의 Blocker가 행 위에 놓이는지. `cargo test -p beetle-render --release --tests`
      캡처(`select-list`, `select-options`, `select-exit`): 상단바 OPTIONS 버튼이 검색창 오른쪽에 겹침 없이 들어간다.
    - 실제 앱(격리 실행 폴더, `PostMessage`로 입력, 실제 커서는 움직이지 않음): 행 클릭 선택, 휠 방향·칸 수,
      OPTIONS 클릭으로 옵션 모달 열림, 모달 안 빈 곳 클릭은 유지, `>` 클릭으로 HI-SPEED 1125 → 1150,
      모달 바깥 클릭은 닫기만 하고 아래 행으로 새지 않음, 검색창 클릭(커서 표시), 검색 중 행 클릭은 검색 종료,
      폴더 `>` 클릭(ALL SONGS → 5 KEYS). 캡처: `scratch/u1a/live-*.png`, `scratch/u1a/final-*.png`.
    - 한계: 합성 입력은 winit의 마우스 나가기 이벤트와 경합해, 커서 위치가 비어 클릭이 버려지는 경우가 있었다.
      앱은 커서 위치가 없으면 클릭을 무시하며 실제 마우스에서는 해당하지 않는다(하네스는 재시도로 우회).
      선택된 행 클릭으로 재생하는 동작과 호버 강조는 실제 앱 캡처로 끝까지 확인하지 못했다.
- **U1b — 결과·키 설정 마우스 + 푸터 버튼** (완료, 메뉴 액션 계층은 보류)
  - [ ] ~~**메뉴 액션 계층**~~ (보류): 게임패드 입력이 없고, 건반 키(S D F Space J K L, Z X C 등)가
        메뉴 단축키(A 자동, R 재생, C 키 설정, J/K 이동)와 겹쳐 공용 번역 계층의 기준이 정해지지 않는다.
        컨트롤러 입력을 실제로 붙이는 시점에 다시 정한다.
  - [x] **결과·키 설정 클릭**: 결과 푸터(SONG SELECT·RETRY·SCREENSHOT), 키 설정 모드 탭(클릭 = 모드 전환),
        건반 버튼(클릭 = 선택, 선택된 건반 다시 클릭 = 입력 대기), 푸터(SET KEY·ADD KEY·CLEAR·PRESET·
        SCRATCH·8K FORM·RESET·BACK). 입력 대기 중 아무 곳이나 클릭하면 ESC와 같이 취소. 키 설정 휠 = 이전/다음 모드.
  - [x] **푸터 키캡 = 버튼**: 선곡 푸터 중 SELECT를 뺀 항목이 클릭으로 같은 동작을 한다(FOLDER는 다음 폴더).
        `handlers/mouse.rs`가 화면별로 나눈다. 게임플레이·로딩·부팅은 여전히 무시.
  - 결과 (2026-10-10):
    - 크기: `beetle-app.exe` 3,015,680 B → 3,019,264 B (+3.5 KB; U0 기준선 대비 +7.1 KB).
    - 검증: `cargo test --workspace` 통과. 새 테스트: 결과 푸터 버튼이 푸터 안에 있고 중심점이 맞는지,
      키 설정의 모드 탭·건반·푸터 위치, 입력 대기 중 푸터 교체, 선곡 푸터 버튼. 렌더 캡처 `result-clear`,
      `keys-7k`, `keys-14k`, `select-list`는 U1a 커밋의 캡처와 바이트 단위로 같다(호버 없음).
    - 실제 앱(격리 실행 폴더, `PostMessage`): 선곡 KEYS 클릭 → 키 설정, 5 KEYS 탭, 건반 두 번 클릭 →
      입력 대기("SET KEY"), 빈 곳 클릭 → 취소, BACK → 선곡. 선곡 AUTO 클릭 → PLAY → 결과(자동 플레이, 36노트 곡),
      결과의 SONG SELECT 클릭 → 선곡. 캡처: `scratch/u1b/live-*.png`.
    - 한계: 합성 입력 경합으로 클릭이 버려져 재시도했다(U1a와 같은 이유). RETRY·SCREENSHOT 버튼과 키 설정
      휠은 실제 앱에서 누르지 않았고, 같은 함수를 호출하는 키 경로와 단위 테스트로만 확인했다.
- **U1c — IME 검색** (완료)
  - [x] **IME**: `Window::set_ime_allowed` + `WindowEvent::Ime`(Preedit/Commit). 조합 중 글자는
        검색창에 밑줄로 표시. 검색창이 열려 있을 때만 IME를 켠다(게임플레이 키 입력 방해 방지).
  - 구현: `crates/beetle-app/src/ime.rs`. 검색창 열림·닫힘은 `set_search_active` 한 곳에서만 바꾸며
    IME 켜짐을 함께 맞춘다. 선곡을 떠나거나 모달이 열리면 `close_search_if_unavailable`가 닫는다(키 입력 전·
    매 프레임). `WindowEvent::Ime`: Preedit → `search_preedit`(검색 결과는 바뀌지 않음), Commit → 질의에
    추가하고 목록 재계산, Disabled → 조합 비움. 검색창 캐럿 사각형은 `Ui::ime_caret`으로 기록하고, 바뀔 때만
    `set_ime_cursor_area`로 넘겨 후보창이 캐럿 아래에 뜨게 한다. 그리기: 질의 뒤에 조합 글자를 밑줄과 함께 두고
    캐럿은 그 뒤에 둔다(같은 줄, 앞쪽 잘라내기 유지).
  - 이중 입력 방지: `KeyEvent::text`(WM_CHAR)와 `Ime::Commit`이 같은 글자를 함께 줄 수 있어, IME가 켜진
    검색창에서는 비ASCII 키 텍스트를 버린다. 조합 중에는 키 텍스트·Backspace·Enter·Escape를 IME에 넘긴다(질의
    불변). ASCII 키 텍스트는 영문 모드 입력이라 그대로 받는다. 근거: winit 0.30 Windows는 WM_IME_COMPOSITION에
    DefWindowProc을 호출하지 않아 조합 결과가 WM_CHAR로 나오지 않는다(`platform_impl/windows/event_loop.rs`).
    `WM_IME_CHAR`가 WM_CHAR로 바뀌는 경우는 소스로만 확인했다.
  - 결과 (2026-10-10):
    - 크기: `beetle-app.exe` 3,019,264 B → 3,023,360 B (+4 KB, +0.14%).
    - 검증: `cargo test --workspace` 통과. 새 테스트: `ime.rs`(커밋 추가·제어문자 제외, 조합 중 질의 불변,
      조합 중 Backspace, ASCII 키 텍스트 규칙, 닫힘 조건), `select.rs`의 `ime_caret_sits_in_the_search_box_after_the_preedit`.
      렌더 캡처 `select-ime-preedit`(질의 "가을" + 조합 "밤", `target/select-ime-preedit.bmp`): 밑줄과 캐럿이
      "밤" 뒤에 놓인다(`scratch/u1c/ime-preedit-top.png`).
    - 실제 앱(격리 실행 폴더, `PostMessage`로만 입력, 실제 커서는 움직이지 않음): `/`로 검색 열기 → "tr" 입력 시
      목록이 필터됨(`live-03-typed-tr.png`). 비ASCII WM_CHAR와 원시 WM_IME_CHAR(U+AC00)는 질의를 바꾸지 않음
      (`live-04`, `live-04b`). Backspace는 한 글자 지움(`live-05`). Escape 두 번으로 지우고 닫음(`live-06`).
      닫은 뒤 A는 AUTO PLAY를 토글(`live-07`). 캡처: `scratch/u1c/live-*.png`.
    - 한계: 실제 한글·일본어 IME 조합은 구동하지 못했다. 합성 메시지에는 IME 컨텍스트가 없어 `Ime::Preedit`/
      `Ime::Commit`이 나오지 않는다. 그래서 조합 밑줄과 Commit 경로는 단위 테스트와 캡처로만 확인했고, 후보창의
      실제 표시 위치는 확인하지 못했다(`set_ime_cursor_area` 호출까지만 확인). 하네스는 `PostMessage`(ANSI)로
      비ASCII WM_CHAR를 보내면 `?`로 바뀌어 `PostMessageW`로 바꿨다.
- **U1d — 문자열 표(한국어)**
  - [ ] **문자열 표**: `strings.rs`(한국어 `&'static str` 표). 화면 코드의 리터럴을 옮긴다.
- **U1e — 화면 전환 + 토스트**
  - [ ] **화면 전환**: 공용 페이드/슬라이드(150~250 ms, `motion.rs` 이징). 판정 타이밍과 무관한
        메뉴 화면 사이에만 적용, Loading→Gameplay는 오디오 시작 전에 끝나게.
  - [ ] **토스트**: 화면 공용 짧은 알림(설정 저장, 재스캔 완료, 끌어다 놓기 결과 등).

### U2 — 옵션 재구성
- [ ] 옵션을 **표 기반**으로: `OptionDesc { id, 그룹, 라벨, 설명, 값 목록/범위, get, set }` 배열.
      처리기·그리기가 같은 표를 순회 → 행 번호 하드코딩 제거.
- [ ] **플레이 옵션**(선곡 화면 패널, Tab): HI-SPEED, 그린 넘버, MODIFIER, GAUGE, LN MODE,
      레인 커버(SUD+/LIFT), AUTO PLAY. 선택한 행의 설명을 패널 하단에 표시.
- [ ] **설정 화면**(새 `AppScreen::Settings`): 화면(모드·해상도·그래픽·FPS·UI 배율),
      소리(볼륨·오디오 버퍼), 입력(키 설정 진입), 판정(오프셋 + **오프셋 측정 도구**:
      박자에 맞춰 키를 눌러 평균 차이를 제안), 레이아웃(플레이필드 위치·스크래치·BGA).
- [ ] HI-SPEED를 그린 넘버(노트 표시 시간 ms)로 교체. `config.dat` 옛 px/s 값 환산 이관 + 테스트.

### U3 — 선곡 개편
- [ ] **폴더 트리**: 최상위 = 전체 / 키 모드 / 레벨(1~N 하위 폴더) / 클리어 램프 / 난이도표(표 →
      레벨 하위 폴더). Enter(또는 클릭)로 들어가고 Back으로 나온다. 상단바에 경로(빵부스러기).
      F1/F3는 "같은 깊이의 이전/다음 폴더"로 유지.
- [ ] **곡 묶음**: 같은 폴더의 채보를 한 행으로, 상세 패널에
      난이도 탭(←/→ 또는 클릭). 선택한 난이도는 곡마다 기억.
- [ ] **정렬·필터 칩**: 상단바에 정렬 드롭다운 + 필터 칩(모드, 레벨 범위, 미플레이/미클리어만).
- [ ] **검색**: IME 입력, 결과 개수 표시, 제목·아티스트·장르 대상.
- [ ] **푸터 축소**: 상황별 키캡 3~5개만, 나머지는 `?`(도움말 오버레이)로.
- [ ] **빈 서재·첫 실행**: 곡 넣는 세 가지 방법(bpm-gui 열기 버튼, 끌어다 놓기, songs 폴더
      열기)과 다시 읽기(F5) 안내. 끌어다 놓기 중 오버레이.
- [ ] 행 정보 정리: 미플레이 행의 "NO PLAY" 반복 대신 흐린 램프 + 빈칸.

### U4 — 게임플레이 HUD
- [ ] **BGA 없을 때 레이아웃**: 빈 상자 대신 점수 패널을 키우거나(판정 그래프·게이지 추이)
      재킷/배경 아트로 채움. BGA OFF 옵션과 같은 경로.
- [ ] 플레이 중 키 안내는 첫 3초만 표시 후 페이드(설정으로 끌 수 있게).
- [ ] **시작 준비 구간**: 첫 노트까지 "READY" + 레인 커버·하이스피드 조정 안내.
      (오디오 클럭 기준, INV-1 영향 없음 — 표시만.)
- [ ] **종료 연출**: FULL COMBO / CLEAR / FAILED 배너 후 결과로 전환.
- [ ] **하이스피드·커버 변경 표시**: 바꾼 값과 그린 넘버를 레인 위에 1초간.

### U5 — 결과
- [ ] 게이지 추이 그래프(플레이 중 게이지 값을 일정 간격으로 기록 — `ScoreTracker` 또는 앱 쪽).
- [ ] 다음 행동: 선곡으로 / 재도전 / **옵션 바꿔 재도전**(플레이 옵션 패널을 결과 위에 열기) / 스크린샷.
- [ ] 마우스로 각 버튼 클릭.

### U6 — 마무리
- [ ] UI 배율 옵션(고DPI·큰 화면에서 메뉴 글자 크기), 판정색 대체 팔레트 검토
- [ ] 화면별 하드웨어·WARP fps 측정, 릴리스 `beetle-app.exe` 크기 측정·기록
- [ ] AGENTS.md 모듈 설명(`AppScreen`에 Settings 추가) 갱신

순서 근거: U1이 없으면 U2~U5에서 마우스·IME·문자열을 화면마다 따로 붙이게 된다.
U2(옵션 분리)가 U3보다 먼저인 이유는 선곡 화면 레이아웃이 옵션 패널 위치에 달려 있어서다.

## 결정 (2026-10-10)

1. **UI 언어: 한국어**(bpm-gui와 통일). 판정명(PGREAT 등)·랭크·EX SCORE 같은 BMS 관례
   용어는 영어 유지. 문자열 표는 언어 하나로 시작한다.
2. **마우스: 메뉴 전체**(선곡·옵션·설정·결과·키 설정, 클릭 + 휠). 게임플레이는 제외.
3. **HI-SPEED: 그린 넘버로 교체**. 설정값은 노트 표시 시간(ms) 기준으로 저장하고,
   `config.dat`의 옛 px/s 값은 저장 당시 해상도·레인 높이로 환산해 이관한다.
4. **곡 묶음: 같은 폴더끼리**. 상세 패널에 난이도 탭.
5. **설정 화면 진입: 새 타이틀 화면 없이 기존 흐름 유지.** 선곡 화면에서 단축키(TAB·O)와 마우스 버튼
   (상단바 OPTIONS)으로 연다. U2에서 설정 화면(`AppScreen::Settings`)이 생기면 이 버튼이 그곳으로 연결된다.

## 남은 결정

- 없음. 설정 화면 진입은 결정 5로 정했다.

## 위험

- **IME**: winit의 IME 이벤트는 창 단위라 게임플레이 중 켜져 있으면 키 입력을 가로챈다 →
  검색창이 열려 있을 때만 허용하고 닫을 때 반드시 끈다.
- **폴더 트리 + 곡 묶음**은 `recompute_filtered_songs`와 커서 인덱스 모델을 바꾼다 →
  순수 함수(`AppState` 밖)로 만들어 단위 테스트를 먼저 쓴다.
- **설정 파일 이관**(HI-SPEED 단위, 새 옵션) → 옛 키 읽기 테스트 추가.
- 바이너리 크기: 문자열 표·새 화면은 수십 KB 수준 예상. 단계마다 측정.
