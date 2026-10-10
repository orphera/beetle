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

### U1 — 입력 기반 (마우스 · 메뉴 액션 · IME · 문자열 · 전환) (완료, 메뉴 액션 계층은 보류)
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
- **U1d — 문자열 표(한국어)** (완료)
  - [x] **문자열 표**: `crates/beetle-render/src/strings.rs`(한국어 `&'static str` 표, 화면별 묶음 `pub const`).
        값이 들어가는 문구는 `{}` 템플릿과 `strings::fill`로 채운다. 화면(`screens/`), `present.rs`, `state.rs`(폴더 이름),
        `main.rs`(창 제목·오류 대화상자)의 문구를 옮겼다. 설정 파일 값(`as_str`)과 키 이름은 바꾸지 않았다.
  - [x] **글자 범위 테스트**: 표의 모든 글자가 내장 KR 폰트(`text::KR_BYTES`)에 있는지 검사한다
        (`every_character_is_in_the_embedded_kr_font`). 없는 글자 `—`, `·`, `↑↓`는 `-`, `/`, 한글 또는 키캡 글자로 바꿨다.
  - [x] **자간**: `theme::caption`의 기본 자간 1 px를 없앴다. 상단 바 제목은 BEETLE 워드마크만 자간을 준다.
        한국어 캡션과 버튼 라벨의 `.tracking`을 제거했다. 남은 자간은 영문 캡(FAST·SLOW, 판정명, STAGE CLEAR, `7K` 탭)에만 있다.
  - 결과 (2026-10-10):
    - 크기: `beetle-app.exe` 3,023,360 B → 3,023,872 B (+512 B, +0.02%).
    - 검증: `cargo test --workspace` 통과. `cargo test -p beetle-render --release --tests` 후 캡처 17장을
      PNG로 바꿔 `scratch/u1d/`에 두고 전부 확인했다(글자 잘림·겹침·두부 글자 없음, 1280x720 푸터 힌트 모두 들어감).
      적용 범위는 `boot-enter`, `select-list`, `select-options`, `select-exit`, `select-empty`, `select-noplay`,
      `loading-enter`, `loading-long`, `play-7k`, `play-paused`, `play-14k`, `result-clear`, `result-failed`,
      `result-auto`, `keys-7k`, `keys-14k`, `keys-9k-rebind`.
    - `keys-9k-rebind.bmp`는 예전 세션에서 만든 파일이 남아 있었고 어떤 테스트도 다시 만들지 않았다.
      `d3d11_keys.rs`에 `Rebind::Replace` 캡처(`9k-rebind`)를 추가해 다시 만들었다.
    - 테스트 픽스처(`tests/d3d11_*.rs`, 화면 단위 테스트)의 라벨·칩·상태 문구를 표 상수로 바꿨다.
      그래야 캡처가 실제 앱 문구를 보여 준다. 테스트 데이터(곡 제목·아티스트)는 그대로다.
    - 새 테스트: `every_character_is_in_the_embedded_kr_font`, `fill_substitutes_placeholders_in_order`.
    - 용어 결정(목록 밖):
      - 라벨: MODIFIER → 모디파이어, GAUGE → 게이지, LN MODE → LN 모드, NOTES → 노트, MODE → 모드,
        PLAYFIELD → 플레이필드, TRACK BGA → 트랙 BGA, BGA는 그대로, START MEASURE → 시작 마디, SCRATCH → 스크래치.
        그룹: 플레이 / 소리 / 레이아웃 / 화면 / 시스템 / 입력 / 세션.
      - 값: ON/OFF → 켜짐/꺼짐, WINDOWED/BORDERLESS/FULLSCREEN → 창 모드/테두리 없는 창/전체 화면,
        판정 위치 CENTER/LEFT/RIGHT → 가운데/왼쪽/오른쪽, 트랙 BGA 값 → 꺼짐 (0%)/낮음 (25%)/보통 (50%)/높음 (75%),
        그래픽 AUTO → 자동(WARP (CPU)는 그대로), UNLIMITED → 무제한, AUTO (LN) → 자동 (LN).
      - 폴더: ALL SONGS → 전체 곡, 5 KEYS … 14 KEYS → 5키 … 14키, BY LEVEL → 레벨별,
        BY CLEAR STATUS → 클리어 상태별, TABLE → 난이도표. 정렬: 제목 / 레벨 / 클리어 / 정확도 / BPM.
        (`SortMode`에 아티스트 정렬이 없어 목록에 넣지 않았다.)
      - 키 설정 탭: `5 KEYS` → `5K`(`theme::mode_label`과 같은 형식). 레이아웃 요약·시작 안내는 한글.
      - 선곡·결과: 미리듣기, 최고 기록, 아직 플레이하지 않음, 회 플레이(`1회 플레이`), 신기록(NEW 태그도 같은 말),
        첫 플레이, 최고 {점수}, 판정·타이밍·오프셋, 기록이 저장되지 않음.
      - 플레이: 콤보, BGA 없음, 일시정지 메뉴(계속하기 / 처음부터 / 곡 선택으로), 힌트 줄 `키 … 1/2 하이스피드 F10/F11 커버 ESC 일시정지`.
      - 종료 대화상자: "BEETLE을 종료할까요?" / "기록과 설정은 이미 저장되어 있습니다." 창 제목 "Beetle - BMS 리듬 엔진"(em dash 대체).
    - 영문으로 남긴 것(이유):
      - 판정명 PGREAT·GREAT·GOOD·BAD·POOR·MISS, FAST / SLOW, EX SCORE, 랭크(AAA…F, MAX), 클리어 램프(PERFECT, FULL COMBO, CLEAR, FAILED 등),
        배너 STAGE CLEAR / STAGE FAILED(램프와 같은 계열로 묶음), 게이지·모디파이어 이름(GROOVE, RANDOM …), LN / CN / HCN, BPM, MIN BP,
        모드 라벨(7K …), 난이도 이름, 키 이름·키캡, 프리셋 식별자(HomeRow …), WARP (CPU), BEETLE 워드마크,
        `M.{}`(마디 표기), 판정 목표 `AAA +38`.
      - 1P / 2P, `+0 ms` 같은 숫자·단위는 그대로.
    - 남은 문제(캡처에서 확인):
      - 결과 화면의 `신기록` 태그는 8 px이라 작다. U5 결과 개편 때 크기를 다시 본다.
      - 10 px 한국어 캡션(선곡의 폴더·정렬 라벨, 로딩의 BPM·노트·모드)이 작게 보인다. U2에서 캡션 크기를 함께 정한다.
      - 선곡 상단 `<` `>` 폴더 라벨과 `TITLE` 값은 캡처 픽스처가 바뀐 뒤 `전체 곡` / `제목`으로 보인다.
      - 자동 플레이 배지 등 실제 앱에서만 보이는 문구는 실제 앱 캡처로 다시 확인해야 한다(이번 작업은 라이브 실행 없이 캡처만 확인).
    - AGENTS.md 4절은 바꾸지 않았다(`beetle-render`의 책임은 그대로, `strings.rs`는 그 안의 데이터 모듈).
- **U1e — 화면 전환 + 토스트** (완료)
  - [x] **화면 전환**: 메뉴 화면이 들어올 때 배경색 덮개가 200 ms 동안 `ease_out_cubic`으로 걷힌다
        (1→0). 적용: Boot→선곡, 선곡↔키 설정, 선곡→로딩, 결과→선곡, 게임플레이→결과.
        게임플레이에는 덮개를 그리지 않고, 로딩→게임플레이는 컷이다(`transition::fades_in`).
        화면 변경은 `transition::ScreenEntry::sync` 한 곳에서만 기록한다(그리기 직전과 루프마다 호출).
        판정과 AudioClock은 건드리지 않는다.
  - [x] **토스트**: `transition::Toast`를 한 번에 하나만 둔다(새 토스트가 교체). 150 ms 슬라이드·페이드 인,
        2.5 초 유지, 300 ms 페이드 아웃. 그리기는 `screens/overlay.rs`(theme 토큰, 패널·아이콘 스프라이트).
        선곡·키 설정·결과에만 그리고, 게임플레이·로딩·부팅에는 그리지 않는다. 클릭 영역은 없다.
        발생 지점: F5 재스캔 완료("곡 N개를 읽었습니다"), P 스크린샷 저장 성공(파일 이름)·실패,
        지원하지 않는 파일 끌어다 놓기, `.bmsp`/`.bms` 열기 실패(곡을 열지 못함), 키 설정 프리셋 변경·초기화(정보).
        "설정 저장" 토스트는 넣지 않았다(설정은 키 설정 화면을 나올 때 조용히 저장된다).
  - [x] 애니메이션 중에만 16 ms 간격으로 깨어난다. 끝난 뒤 한 프레임을 더 그려 덮개와 토스트를 지운 다음 다시 잠든다.
  - 결과 (2026-10-10):
    - 크기: `beetle-app.exe` 3,023,872 B → 3,026,944 B (+3 KB, +0.10%). 기준값은 U1d 결과의 기록이다.
    - 검증: `cargo test --workspace` 통과. 새 테스트: `transition.rs`(페이드 곡선과 단조성, 게임플레이에는
      덮개가 없다는 규칙, 화면이 바뀔 때만 시각이 리셋됨, 토스트 타임라인, 새 토스트가 이전 것을 교체,
      토스트는 메뉴에서만 표시). 렌더 캡처 `select-toast`, `select-fade-mid`(`scratch/u1e/`).
      `cargo test -p beetle-render --release --tests` 통과.
    - 실제 앱(격리 실행 폴더, `PostMessageW`로 키 입력, `PrintWindow`로 캡처, 실제 커서는 움직이지 않음):
      F5 재스캔 후 선곡으로 돌아오며 덮개가 걷히는 중 토스트가 올라오고(`scratch/u1e/live/02-f5-toast-00.png`),
      안정된 뒤 "곡 216개를 읽었습니다"가 보인다(`02-f5-toast-03.png`). 4 초 뒤 사라짐(`03-f5-toast-gone.png`).
      F12 키 설정 덮개 페이드(`04-keys-fade-00.png`), F1 프리셋 토스트(`05-keys-preset-toast.png`), 3.5 초 뒤
      사라짐(`06-keys-preset-toast-gone.png`). 종료 후 `beetle-app.exe`가 남지 않았다.
    - 한계: P 스크린샷 토스트와 끌어다 놓기 오류 토스트는 실제 앱에서 누르지 않았다(같은 함수 경로를 단위
      테스트와 코드로만 확인). 게임플레이→결과 페이드와 게임플레이 중 덮개 없음은 실제 앱으로 보지 못했다
      (`gameplay()`는 덮개 호출이 없고 `fades_in(Gameplay)`는 false). 로딩→게임플레이 컷도 실제 앱으로는 확인하지 않았다.
    - 알려진 점: 프리셋 토스트의 이름은 키 설정 상단의 프리셋 표시와 같은 `6K (A S D L ; ')`라서 모드 표시와 겹친다.
      문구 정리는 U2 옵션 개편 때 본다. 토스트는 3초 동안 모드 탭 일부를 가린다(의도된 임시 표시).

### U2 — 옵션 재구성 (완료)
U2는 세 단계로 나눈다. U2a에서 옵션 구조를 바꾸고, U2b·U2c는 값의 의미와 측정을 다룬다.

- **U2a — 옵션 표 · 플레이 옵션 패널 · 설정 화면** (완료)
  - [x] **옵션 표**: `crates/beetle-app/src/options_table.rs`. `PLAY_OPTIONS`(플레이 옵션 패널)와 `SETTINGS`
        (설정 화면) 두 표가 그리기와 입력을 함께 맡는다. 행은 표의 인덱스이고 행 번호 상수는 없다.
        행마다 `OptionId`, 그룹, 열, 라벨, 설명 한 문장이 있다. 값 읽기(`value`)와 한 칸 바꾸기(`step`)는
        `OptionId`로 분기하고, ENTER 동작(`activation`)은 키 배치 한 곳뿐이다.
  - [x] **플레이 옵션 패널**(선곡, TAB·O·상단 OPTIONS 버튼): 하이스피드, 레인 커버, 모디파이어, 게이지,
        LN 모드, 자동 플레이, 시작 마디. 한 열이고 아래에 선택한 행의 설명 카드를 둔다.
  - [x] **설정 화면**(`AppScreen::Settings`, `screens/settings.rs`, `handlers/settings.rs`): 두 열.
        화면(화면 모드·해상도·그래픽·목표 FPS), 소리(전체 볼륨), 판정(판정 오프셋), 레이아웃(플레이필드·
        스크래치·BGA·트랙 BGA), 입력(키 배치). 키 배치는 ENTER나 클릭으로 키 설정을 연다. 키 설정에서
        ESC로 나오면 설정 화면으로 돌아간다(`key_config_return`). ESC로 나오면 저장하고 선곡으로 간다.
        그래픽을 바꾼 채 나오면 토스트 "재시작 후 적용됩니다".
  - [x] **진입**: 선곡 상단 바의 설정 버튼(F4, `HitId::OpenSettings`)과 푸터 `F4 설정`. F4는 선곡에서만
        쓰고 게임플레이의 F4(하이스피드 감소)와는 화면이 달라 충돌하지 않는다.
  - [x] **옛 두 열 모달 제거**: `draw_options_modal`은 한 열이 되었고, `OPTION_SECTIONS`·
        `OPTION_COLUMN_BREAK`·`OPTION_ROWS`·`KEY_LAYOUT_ROW`·`option_modal_rows`는 없앴다.
        상단 바 버튼은 `HitId::Settings`에서 `HitId::PlayOptions`(옵션)와 `HitId::OpenSettings`(설정)로 나눴다.
  - [x] **문자열**: 새 라벨·설명 문장은 `strings.rs`에 있고, 글자 범위 테스트를 통과한다.
  - 행 이동 표(옛 16행 → 새 위치):
    | 옛 행 | 항목 | 새 위치 |
    |---|---|---|
    | 0 | 하이스피드 | 플레이 옵션 |
    | 1 | 모디파이어 | 플레이 옵션 |
    | 2 | 게이지 | 플레이 옵션 |
    | 3 | LN 모드 | 플레이 옵션 |
    | 4 | 판정 오프셋 | 설정 · 판정 |
    | 5 | 전체 볼륨 | 설정 · 소리 |
    | 6 | 플레이필드 | 설정 · 레이아웃 |
    | 7 | BGA | 설정 · 레이아웃 |
    | 8 | 트랙 BGA | 설정 · 레이아웃 |
    | 9 | 화면 모드 | 설정 · 화면 |
    | 10 | 해상도 | 설정 · 화면 |
    | 11 | 그래픽 | 설정 · 화면 |
    | 12 | 목표 FPS | 설정 · 화면 |
    | 13 | 키 배치 | 설정 · 입력 (ENTER: 키 설정) |
    | 14 | 자동 플레이 | 플레이 옵션 · 세션 |
    | 15 | 시작 마디 | 플레이 옵션 · 세션 (설계 결정: 곡마다 정하는 값이므로 패널에 둠) |
    새로 추가: 레인 커버(플레이 옵션, F10·F11과 같은 0–80 %, 5 % 단위), 스크래치(설정 · 레이아웃,
    선택한 곡의 모드가 5K·7K·8K일 때만 적용, 아니면 "해당 없음").
  - 범위에서 뺀 것: 원래 U2 목록의 UI 배율은 U6에 그대로 두고, 오디오 버퍼는 이번에 넣지 않았다.
  - 결과 (2026-10-10):
    - 크기: `beetle-app.exe` 3,026,944 B(U1e 결과) → 3,033,088 B (+6,144 B, +0.20%).
    - 검증: `cargo test --workspace --release` 전부 통과, `cargo fmt --all -- --check` 통과. 새 테스트:
      `options_table.rs`(id 유일, 설명 있음, 두 표에 같은 행 없음, 열·그룹 규칙, ENTER 동작은 키 배치뿐,
      단계 함수의 범위와 왕복, FPS 순환), `settings.rs`(행·ESC 히트 영역, 한 배치), `select.rs`의 히트 영역
      (옵션·설정 버튼 포함), `transition.rs`(설정의 페이드·토스트). 글자 범위 테스트 통과.
      렌더 캡처(`scratch/u2a/`): `select-options.png`(한 열 패널과 설명 카드), `settings-display.png`
      (재시작 필요 값과 설명), `settings-keys.png`(키 배치 행 선택, 해당 없음 값).
    - 실제 앱(격리 실행 폴더 `scratch/ux-before/run`, `PostMessageW`만 사용, 실제 커서 불변):
      - 상단 바 설정 버튼 클릭 → 설정 화면(`02-settings-by-click.png`).
      - 전체 볼륨 ↓4 →: 100% → 105%, 도움말 카드(`03-volume-plus.png`). ESC로 나오면 `config.dat`의
        `master_volume=1.05`로 저장되고 선곡으로 돌아온다(`04-back-after-esc.png`).
      - TAB → 플레이 옵션 패널, 설명 카드(`05`, `06-play-panel-lanecover.png`). 레인 커버 0% → 5%
        (`lane_cover_ratio=0.05` 저장, `07`).
      - F4 → 설정, 키 배치 행에서 ENTER → 키 설정 6K(`08`, `09-keyconfig-from-settings.png`). 키 설정의 ESC는
        설정으로 돌아오고 같은 행이 선택되어 있다(`10-back-to-settings.png`).
      - 그래픽을 WARP로 → "WARP (CPU) (재시작 후 적용)"(`11-graphics-changed.png`), ESC → 토스트
        "재시작 후 적용됩니다"(`12-restart-toast.png`). 실행 뒤 그래픽은 자동으로 되돌려 저장했다.
      - 종료 후 `beetle-app.exe` 프로세스 0개.
    - 알려진 점:
      - 키 배치 값이 `6K  6K (A S D L ; ')`로 모드 표기가 두 번 나온다. 프리셋 이름에 이미 모드가 들어 있어
        U1 때 옮긴 표시를 그대로 둔 것이다. U2b 전에 한 번만 정리한다.
      - 합성 클릭은 winit의 "커서 나감"과 경합해 버려질 수 있다. 이동과 누름을 한 묶음으로 보내면 된다
        (`scratch/u2a/live.ps1`). 마우스 휠 스크롤·화살표 버튼 클릭은 실제 앱에서 누르지 않았다(같은 함수
        경로를 키와 단위 테스트로만 확인).
      - 단위 테스트는 `AppState`를 만들 수 없어(창이 필요) 행별 값·단계는 순수 함수로만 확인했다.
        행마다 `step`이 실제 상태를 바꾸는지는 실행 중 확인한 행(볼륨, 레인 커버, 그래픽, 키 배치)에서만 본다.
- **U2b — HI-SPEED를 그린 넘버로** (결정 3)
  - [x] 하이스피드를 노트 표시 시간(ms, 그린 넘버)으로 저장하고 표시한다. 지금 값은 px/s 그대로다.
  - [x] `config.dat`의 옛 px/s 값을 저장 당시 해상도·레인 높이로 환산해 옮긴다. 환산 테스트 추가.
  - [x] 설명 문장과 `HELP_HI_SPEED`를 새 단위에 맞춘다.
  - 결과 (2026-10-10):
    - 환산식(`crates/beetle-render/src/skin.rs`): 720 단위 기준 판정선까지 거리 592(= 616 − 24), 필드 높이 672.
      레인 커버는 필드 위에서 `672 × 커버`만큼 가리므로 보이는 길이는 `592 − 672 × 커버`.
      `px/s = 보이는 길이 × 1000 / 그린 ms`, 역은 같은 식을 뒤집는다. 필드 위치·키 모드와 무관한 값이라 인자가 없다.
      `play.rs`는 px/s를 그대로 쓰므로 노트 이동 그림은 바뀌지 않는다(같은 실효 속도 기준).
    - 기본값: 커버 0%에서 기존 기본 400 px/s와 같은 1480 ms. 설정 범위 100–2000 ms, ←/→ 10 ms 단위.
    - 레인 커버를 바꾸면 그린 넘버는 그대로 두고 px/s를 다시 계산한다(F10/F11, 플레이 옵션 패널).
      F3/PageUp/1은 그린 −10 ms(빨라짐), F4/PageDown/2는 +10 ms.
    - 이관: `green_ms=`가 없고 `hi_speed=`가 있으면 저장된 `lane_cover_ratio`로 환산해 10 ms 단위로 반올림한다.
      저장은 `green_ms=`만 쓴다. 두 줄이 모두 있으면 `green_ms`가 이긴다. 10 ms 반올림으로 속도가 최대 약 1% 달라질 수 있다.
    - 표시: 플레이 옵션 행 "그린 넘버 · 490 ms", 선곡 칩 "그린 490", 플레이 중 키 안내 "1/2 그린 넘버".
    - 리플레이·점수 기록에는 하이스피드가 저장되지 않아 형식은 바꾸지 않았다.
    - 키 배치 표시 중복 정리: 프리셋 이름에 모드가 이미 있으면 모드를 다시 붙이지 않는다(`KeyPreset::layout_name`).
      옵션 행, 키 배치 토스트 모두 적용.
    - 검증: `cargo test --workspace`, `cargo test -p beetle-render --release --tests` 통과, `cargo fmt --all -- --check` 통과.
      새 테스트: 환산 값·왕복·커버 범위(`skin.rs`), 옛 파일 이관·우선순위·범위 밖 값(`config.rs`), 단계 10 ms(`options_table.rs`),
      프리셋 이름(`input.rs`). 글자 범위 테스트 통과.
      실제 앱(격리 `scratch/ux-before/run`, 옛 `hi_speed=1150.0` / `lane_cover_ratio=0.05`): 기동 후 `green_ms=490`,
      `hi_speed` 줄 제거. 패널 커버 5 → 15%에서 그린 490 유지. 7K 자동 플레이에서 F10 3회로 커버 30%, 그린 490 유지(캡처는 저장소 밖 작업 폴더에 두었다).
    - 크기: `beetle-app.exe` 3,033,088 B → 3,033,600 B (+512 B).
    - 알려진 점: 실제 화면 위치를 재는 자동 비교는 하지 않았다(환산식은 필드 기하에서 유도). 키 배치 상세 화면의 프리셋 이름은 모드를 따로 표시하므로 그대로 뒀다.
- **U2c — 판정 오프셋 측정 도구** (완료)
  - [x] **진입**: 설정 · 판정의 판정 오프셋 행에서 ENTER(선택한 행의 클릭도 같다)로 측정 화면을 연다. 화살표는 여전히 값을 바꾼다. 측정 중에는 F6–F9 단축키와 휠이 설정을 바꾸지 않는다.
  - [x] **순수 로직** `crates/beetle-app/src/calibration.rs`: 박 일정, 가장 가까운 박 맞추기, 이상값 제외, 평균·편차, 제안값의 반올림과 범위. 오디오 없이 단위 테스트한다.
  - [x] **클릭 소리**: 코드로 만든 한 종류(1200 Hz, 30 ms, 6 ms 지수 감쇠). 측정을 열 때 `SampleBank`에 등록하고(INV-3), 측정 전용 `AudioEngine`으로 `PlaySample` 명령을 보낸다. 오디오 콜백과 믹서는 바꾸지 않았다. 콜백에 새 락·할당은 없고, 명령 큐(rtrb)만 쓴다.
  - [x] **박 일정**: 120 BPM(0.5 s 간격). 첫 클릭은 측정을 연 오디오 시각 + 1.0 s다. k번째 클릭의 공칭 시각은 `start + k × 0.5 s`이고, 매 프레임 `tick()`이 공칭 시각이 지난 클릭을 큐에 넣는다. 게임플레이 키음과 같은 경로다.
  - [x] **입력 시각**: 키를 처리하는 순간의 `AudioClock::current_time_seconds()`에서 가장 가까운 박의 공칭 시각을 뺀다(ms). 준비 박 0–3번의 입력은 세지 않는다. ±150 ms 밖의 입력과 같은 박에 두 번째로 누른 입력은 무시한다. 입력은 스페이스, 엔터, 선택한 곡 모드의 레인 키이며 반복 입력은 뺀다. 16개를 모으면 측정이 끝나고 클릭이 멈춘다. R은 다시, ESC는 취소, 완료 뒤 ENTER는 적용이다.
  - [x] **이상값 제외**: 중앙값에서 `3 × max(1.4826 × MAD, 10 ms)`보다 먼 입력은 평균에서 뺀다(입력이 4개 미만이면 하지 않는다). 10 ms 바닥값은 오디오 시계가 장치 버퍼 단위로 움직여, 입력이 이웃한 두 단계에 걸치는 경우를 살리려고 둔 것이다.
  - [x] **부호 규약**: 판정은 `judged_time(audio, offset) = audio + offset / 1000`을 노트 시각과 비교한다. 게임플레이의 두 곳(`gameplay.rs`, `handlers/gameplay.rs`)이 이 함수를 쓴다. 평균 오차가 +20 ms(늦게 누름)이면 제안값은 -20 ms다. 테스트 `suggestion_cancels_a_late_press_in_the_real_judge`(`calibration.rs`)는 실제 `JudgeEngine`으로 같은 입력을 오프셋 0과 제안값에서 각각 판정해, +20 ms가 0 ms가 되는 것을 확인한다.
  - [x] **범위와 단계**: 제안값은 1 ms 단위로 반올림하고 -100…+100 ms에 맞춘다. 두 값은 `JUDGE_OFFSET_MAX_MS`, `JUDGE_OFFSET_STEP_MS`로 설정 행과 공유한다.
  - [x] **적용·취소**: 적용(ENTER)은 판정 오프셋을 제안값으로 바꾸고 저장한 뒤 토스트를 띄우고 설정으로 돌아간다. 다시(R)는 새 측정을 시작한다. 취소(ESC)는 아무것도 바꾸지 않는다. 측정 세션을 놓으면 오디오 엔진이 닫힌다.
  - [x] **화면**: `crates/beetle-render/src/screens/calibrate.rs`(`draw_calibrate`). 준비·측정·완료·오디오 없음 네 단계, 박 표시기(오디오 시각 기준으로 점멸), 진행·평균·편차 카드, -150…+150 ms 선에 입력마다 표시(제외된 입력은 흐리게). 색은 theme 토큰만 쓰고, 문자열은 `strings.rs`에 있다. 측정 중에는 프레임을 16 ms로 깨운다(`about_to_wait`).
  - 알려진 한계:
    - 오디오 시계는 장치 버퍼 단위로 늘어나므로 입력 시각의 분해능은 버퍼 한 칸(대략 10–20 ms)이다. 입력이 한 칸에 모두 들어가면 편차가 0.0 ms로 나온다. 판정도 같은 시계를 쓰므로 판정과 같은 분해능이다.
    - 클릭 소리는 큐에서 처리되는 시점에 나오므로 공칭 시각보다 조금 늦고, 그 지연은 일정하지 않다(버퍼 한 칸 안). 이 지연은 제안값에 섞인다.
    - 마우스 클릭은 측정 입력으로 세지 않는다. 키만 센다.
  - 결과 (2026-10-10):
    - 검증: `cargo test --workspace` 통과, `cargo test -p beetle-render --release --tests` 통과, `cargo fmt --all -- --check` 통과. 새 테스트: `calibration.rs` 13개(박 일정, 매칭, 연속 입력, 이상값, 제안값, 펄스, 실제 판정 부호 규약), `screens/calibrate.rs` 3개(히트 영역, 한 배치), `options_table.rs` 활성화 규칙(판정 오프셋 행 추가). 글자 범위 테스트 통과.
    - 캡처(WARP, `scratch/u2c/`): `settings-calibrate.png`(측정 중, 준비 뒤 9번째 입력, 제외 입력 하나), `settings-calibrate-done.png`(완료, 제안값과 적용·다시·취소). 완료 캡처는 표시용으로 마크를 9개만 넣었다.
    - 실제 앱(격리 `scratch/ux-before/run`, PostMessageW와 PrintWindow만 사용, 실제 커서 불변): 설정 판정 행에서 ENTER → 준비 → 측정 → 완료 → ENTER 적용(토스트 "판정 오프셋을 -40 ms로 맞췄습니다") → `config.dat`에 `judge_offset_ms=-40.0`. 같은 방법으로 다시 열고 ESC로 취소하면 값은 그대로다. 종료 후 `beetle-app.exe` 프로세스 0개. 입력은 PowerShell이 0.5 s 격자에 맞춰 보낸 것이라 측정값(실행마다 -40, -42, -52 ms)은 정확도를 보여 주지 않는다.
    - 크기: `beetle-app.exe` 3,033,600 B(U2b 결과) → 3,050,496 B (+16,896 B, +0.56%). `bpm-gui.exe`는 4,677,120 B(이전 빌드, 같은 커밋 기준으로 다시 재지 않음) → 4,832,256 B. 공유 렌더 코드가 들어가 커졌다.

### U3 — 선곡 개편
U3는 네 단계로 나눈다. U3a에서 폴더 구조를 바꾸고, U3b는 곡 묶음, U3c는 정렬·필터·검색, U3d는 안내 문구를 다룬다.

- **U3a — 폴더 트리 · 빵부스러기** (완료)
  - [x] **폴더 트리**: `crates/beetle-app/src/folders.rs`(순수 모델, 단위 테스트). 최상위 = 전체 곡 / 키 모드 / 레벨 / 클리어 램프 / 난이도표(표 → 레벨 하위 폴더). 곡이 없는 폴더는 뺀다. 난이도표 레벨은 표의 순서대로 `심볼+레벨`(`sl12`)로 적는다. 키 모드는 4K·6K·8K를 포함한다.
  - [x] **목록 모델**: 트리는 `recompute_entries`에서 라이브러리·기록·난이도표로 한 번 만든다. 화면 행은 `folders::ListEntry`(`Folder { id, label, count }` / `Song(usize)`)이고, 그리기 쪽은 `SelectRow`(`Song` / `Folder`)로 받는다. 곡 묶음(U3b)은 두 열거형에 `Group` 변형을 더하고, 행 높이·히트 인덱스(`HitId::ListRow`)는 그대로 쓴다.
  - [x] **키·마우스**: ↑↓ / J K 이동. ENTER·SPACE는 폴더면 열고 곡이면 재생(`activate_selected`), → 는 폴더를 연다. ← · BKSP · ESC는 한 단계 위로 간다(루트에서 ESC는 종료 대화상자). F1 / F3와 상단 `<` `>`는 같은 깊이의 이전·다음 폴더로 순환한다(루트에서는 F3 = 첫 최상위, F1 = 마지막). 빵부스러기(`HitId::Crumb`)를 클릭하면 그 단계로 간다. 폴더 줄은 곡 줄과 같은 규칙으로 클릭한다(한 번 선택, 선택된 줄을 다시 클릭하면 연다).
  - [x] **커서**: 한 단계 위로 가면 방금 나온 폴더 줄에 커서가 간다. 폴더 줄에서는 미리듣기·자켓·재생·리플레이가 없다(`current_selected_song`이 `None`).
  - [x] **빵부스러기**: 상단바 `폴더 < 전체 > 레벨 > 12 >`. 좁으면 앞쪽 단계부터 `…`로 줄이고, 마지막 단계는 `fit`으로 자른다. 하단 키 안내는 폴더 안에서만 `BKSP 뒤로`를 보인다.
  - [x] **검색**: 리프 폴더 안에서는 그 안에서만 찾고, 가지 폴더에서는 전체 곡에서 찾는다(결과는 곡 줄). 검색 중에는 폴더 줄이 나오지 않는다.
  - [x] **설정 저장**: `config.dat`의 `folder_path=level/12`(id를 `/`로 이어 씀, 번역된 이름을 쓰지 않음). 기본값 `all`(전체 곡). 저장 폴더가 없어졌으면 가장 가까운 상위 폴더로 간다. 라이브러리를 아직 읽지 않은 동안은 저장된 값을 그대로 둔다(첫 레이아웃이 빈 트리로 돌아서 생긴 버그를 막음). 옛 카테고리 설정 키는 없었으므로 이관할 것이 없다.
  - [x] **문자열**: `strings.rs`의 `FOLDER_*`(전체 곡·키 모드·레벨·클리어 램프·난이도표·루트 `전체`), `FOLDER_SONGS`·`FOLDER_COUNT`, `FOLDER_OPEN_HINT`, `FOOTER_OPEN`. 모드 태그(7K)와 램프 이름(PERFECT)은 영문 그대로다.
  - [ ] **상세 패널의 클리어 램프 분포**: 넣지 않았다(곡 수만 보인다). 필요하면 U3c에서 함께 본다.
  - 결과 (2026-10-10):
    - 테스트: `folders.rs` 16개(트리 모양, 빈 폴더 생략, 개수, 난이도표 레벨 순서, 리프·가지 목록, 검색, 저장 폴더 대체, 형제 이동 순환, 빵부스러기, 설정 형식, 커서 복귀). `config.rs`에 기본값·읽기 테스트. `d3d11_select.rs`에 폴더 캡처 3장(`select-folder-root`, `select-folder-level`, `select-folder-leaf`). `cargo test --workspace`, `cargo test -p beetle-render --release --tests` 통과.
    - 실제 앱(격리 `scratch/ux-before/run`, `PostMessageW`로 키, `PrintWindow`로 캡처, 실제 커서는 움직이지 않음): 시작 시 저장된 폴더(전체 곡) 복원, ESC → 루트(`a2`), ↓↓ ENTER → 레벨 목록(`a4`), ↓×12 → 12(`b2`), ENTER → 12의 곡(`b3`), F3 → 13(`b4`, 빵부스러기 `전체 > 레벨 > 13`), 저장 폴더 `level/13`으로 재시작하면 그 곡 목록이 열림(`c1`), 리프에서 ENTER → 곡 로딩 시작(`c4`). 종료 후 `beetle-app.exe` 프로세스 없음.
    - 한계: **빵부스러기 클릭은 실제 앱에서 확인하지 못했다.** 이번 세션에서는 합성 마우스 입력(이동·누름, 이동 뒤 반복 이동, 호버 포함)이 앱 창에 반영되지 않았다(행 클릭도 같다). U1·U2 기록에서는 같은 방식이 통했으므로 하네스 쪽 원인으로 보지만 확인하지 못했다. 클릭 처리는 키의 `go_to_crumb`를 그대로 부르고 히트 영역은 `select.rs` 테스트로 확인했다.
    - 폴더 행 탭의 색은 `theme::BLUE` 한 가지다. 곡 행의 난이도 색과 겹치지 않게 하려고 골랐다.
    - 크기: `beetle-app.exe` 3,050,496 B(U2c 기록) → 3,070,464 B (+19,968 B, +0.65%). 이번 작업 전 빌드를 다시 재지 않아 기준값은 U2c 기록이다.
- **U3b — 곡 묶음** (예정)
  - 같은 폴더의 채보를 한 행으로 묶고, 상세 패널에 난이도 탭(←/→ 또는 클릭)을 둔다. 선택한 난이도는 곡마다 기억한다(결정 4).
  - 구현 메모: `ListEntry::Group`과 `SelectRow::Group`을 더한다. 트리의 리프는 곡 인덱스 목록을 그대로 쓴다.
- **U3c — 정렬 · 필터 칩 · 검색** (예정)
  - 상단바 정렬 드롭다운, 필터 칩(모드, 레벨 범위, 미플레이·미클리어만). 검색 결과 개수 표시(검색은 IME 적용 완료, U1c).
  - 행 정보 정리: 미플레이 행의 `기록 없음` 반복 대신 흐린 램프 + 빈칸.
- **U3d — 푸터 · 빈 서재 · 끌어다 놓기** (예정)
  - 푸터 키캡 3~5개만 보이고 나머지는 `?`(도움말 오버레이).
  - 빈 서재·첫 실행: 곡을 넣는 세 가지 방법(bpm-gui 열기 버튼, 끌어다 놓기, songs 폴더 열기)과 다시 읽기(F5) 안내. 끌어다 놓기 중에는 오버레이를 띄운다.

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
