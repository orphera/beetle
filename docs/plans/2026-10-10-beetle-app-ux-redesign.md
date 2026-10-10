# beetle-app UI/UX 개편 계획 (2026-10-10)

상태: **완료** (U0–U6, 2026-10-10). 남은 일은 맨 아래 U6 절의 「남은 일」.

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
18. 결과: 게이지 추이 그래프 없음. 다음 행동이 Enter/R/P 세 개뿐(옵션 바꿔 재도전 없음). → U5에서 해결.
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
      - 결과 화면의 `신기록` 태그는 8 px이라 작다. → U5에서 9 px로 맞추고, 램프 태그는 `램프 갱신`으로 바꿨다.
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

### U3 — 선곡 개편 (완료)
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
  - [x] **상세 패널의 클리어 램프 분포**: 넣지 않았다(곡 수만 보인다). U3d에서 넣었다.
  - 결과 (2026-10-10):
    - 테스트: `folders.rs` 16개(트리 모양, 빈 폴더 생략, 개수, 난이도표 레벨 순서, 리프·가지 목록, 검색, 저장 폴더 대체, 형제 이동 순환, 빵부스러기, 설정 형식, 커서 복귀). `config.rs`에 기본값·읽기 테스트. `d3d11_select.rs`에 폴더 캡처 3장(`select-folder-root`, `select-folder-level`, `select-folder-leaf`). `cargo test --workspace`, `cargo test -p beetle-render --release --tests` 통과.
    - 실제 앱(격리 `scratch/ux-before/run`, `PostMessageW`로 키, `PrintWindow`로 캡처, 실제 커서는 움직이지 않음): 시작 시 저장된 폴더(전체 곡) 복원, ESC → 루트(`a2`), ↓↓ ENTER → 레벨 목록(`a4`), ↓×12 → 12(`b2`), ENTER → 12의 곡(`b3`), F3 → 13(`b4`, 빵부스러기 `전체 > 레벨 > 13`), 저장 폴더 `level/13`으로 재시작하면 그 곡 목록이 열림(`c1`), 리프에서 ENTER → 곡 로딩 시작(`c4`). 종료 후 `beetle-app.exe` 프로세스 없음.
    - 한계: **빵부스러기 클릭은 실제 앱에서 확인하지 못했다.** 이번 세션에서는 합성 마우스 입력(이동·누름, 이동 뒤 반복 이동, 호버 포함)이 앱 창에 반영되지 않았다(행 클릭도 같다). U1·U2 기록에서는 같은 방식이 통했으므로 하네스 쪽 원인으로 보지만 확인하지 못했다. 클릭 처리는 키의 `go_to_crumb`를 그대로 부르고 히트 영역은 `select.rs` 테스트로 확인했다.
    - 폴더 행 탭의 색은 `theme::BLUE` 한 가지다. 곡 행의 난이도 색과 겹치지 않게 하려고 골랐다.
    - 크기: `beetle-app.exe` 3,050,496 B(U2c 기록) → 3,070,464 B (+19,968 B, +0.65%). 이번 작업 전 빌드를 다시 재지 않아 기준값은 U2c 기록이다.
- **U3b — 곡 묶음** (완료)
  - [x] **묶는 기준**: 같은 곡 폴더와 같은 기본 제목(끝의 `[...]`·`(...)`를 뗀 제목)인 채보를 한 행으로 묶는다
        (`folders.rs`의 `song_folder`, `base_title`, `group_key`). 폴더 안에 곡이 여럿 있어도 제목이 다르면 섞이지 않는다
        (결정 4를 한 단계 좁힘). 패키지 안 채보는 패키지 경로 + 패키지 안 디렉터리가 폴더다. 데모는 묶지 않는다.
        리프와 검색 결과에 같은 규칙을 쓴다. 채보가 하나뿐이면 일반 곡 행이다.
  - [x] **순서와 자리**: 묶음 안 채보는 레벨, 그다음 제목 순이다. `#DIFFICULTY` 값은 `SongMetadata`에 없어 쓰지 않았다
        (넣으려면 곡 캐시 형식을 바꿔야 한다). 난이도 색은 `theme::level_tier`(레벨의 함수)를 그대로 쓴다.
        묶음 행은 첫 채보의 정렬 자리에 놓인다. 채보를 바꿔도 행이 움직이지 않는다.
  - [x] **데이터 모델**: `ListEntry::Group { key, title, charts, selected }`, `SelectRow::Group { title, charts, selected }`.
        `ListEntry::song()`이 묶음의 선택 채보를 돌려주므로 재생·리플레이·미리듣기·재킷은 기존 경로를 그대로 따른다.
  - [x] **키**: 묶음 행에서 ←/→는 채보를 바꾼다(끝에서 순환). BKSP·ESC는 한 단계 위로 간다. 폴더 행과 묶지 않은 곡 행의
        ←/→는 U3a와 같다(← 위, → 폴더 열기).
  - [x] **클릭**: 묶음 행의 칩과 상세 패널 위 난이도 탭(`HitId::ChartTab { row, pos }`)이 키와 같은 `choose_chart`를 부른다.
        칩은 `종류 레벨`(예: `7K 12`) 글자, 선택한 채보는 채워서 표시한다.
  - [x] **기억**: 묶음 키마다 고른 채보를 `AppState::chart_choice`에 둔다. 기본(가장 낮은 레벨)으로 돌아가면 지운다.
        **config.dat에 저장한다**: `chart_choice=<묶음 키 16진>:<채보 ID>` 줄, 기본이 아닌 선택만, 키 순서로 정렬.
        행 수가 묶음 중 고쳐 쓴 것만이라 파일이 작다. 폴더를 옮기고 재시작해도 유지된다.
  - [x] **상세 패널**: 묶음이면 맨 위에 난이도 탭 줄(22 px, 그 아래로 내용이 12 px 내려간다). 같은 `detail_panel`을 쓴다.
  - [x] **개수**: 하단 `N / M 곡`은 행 기준(묶음은 한 행)이다. 폴더 상세에 채보 수는 넣지 않았다(곡 수만 보인다).
  - 결과 (2026-10-10):
    - **Step 0 (마우스 회귀 점검): 앱 회귀가 아니다.** HEAD(01ec0fb)에서 이동·누름·뗌을 쉼 없이 한 묶음으로 보내면
      설정 버튼과 곡 행 클릭이 모두 반영된다(`scratch/u3b/s1-after-settings-click.png`, `s2-A-row2-nopause.png`).
      이동 뒤 150 ms를 쉬고 누르면 누름이 버려지고(`s3-B-row3-pauses.png`, 행이 그대로), 이동만 보내고 400 ms를 기다리면
      호버 강조도 보이지 않는다(`s4-hover-row1.png`). 이동 직후 쉼 없이 누르면 선택된다(`s5`). 원인은 하네스의 타이밍으로 보인다.
      실제 커서가 창 밖에 있어 Windows가 WM_MOUSELEAVE를 보내고, 쉬는 동안 이 메시지가 도착하면 앱의 커서 위치가 비고 앱은
      커서가 없는 누름을 버린다. 이 가설은 로그로 확인하지 않았다(디버그 로그·워크트리 없이 결과만 비교). U3a의 실패도 같은 방식(이동과
      누름 사이에 쉼)이었다. 앞으로 하네스는 이동·누름·뗌을 한 묶음으로 보낸다.
    - 크기: `beetle-app.exe` 3,070,464 B(U3a 기록) → 3,090,432 B (+19,968 B, +0.65%). 기준은 U3a 기록이고 HEAD를 다시 빌드해 재지 않았다.
    - 검증: `cargo test --workspace`, `cargo test -p beetle-render --release --tests`, `cargo fmt --all -- --check` 통과.
      새 테스트: `folders.rs`(`base_title`, `song_folder`, `group_key`, 묶음 순서·자리, 기억한 채보와 데모, 리프·검색 묶음과 `row_showing`),
      `config.rs`(`chart_choice` 왕복과 잘못된 줄 무시). 렌더 캡처 `select-group-7k`, `select-group-14k`
      (`scratch/u3b/captures/`): 묶음 행의 칩, 선택한 칩 채움, 상세 패널의 난이도 탭 줄.
    - 실제 앱(격리 `scratch/ux-before/run`, PostMessageW로 키·클릭, PrintWindow 캡처, 실제 커서 불변):
      `baby` 검색 → Baby 묶음 한 행(8K 1·6·13·17·21, 5채보, `scratch/u3b/live/p2-01`). → 키로 1 → 6(`04-right-chart1.png`,
      `chart_choice` 줄 생김), ← 로 되돌리면 줄이 사라짐(`05`). 칩 클릭 8K 13, 상세 탭 클릭 8K 17(`p2-02`, `p2-03`).
      ENTER → 로딩 카드 OVERJOY 17 · Baby(`p2-04`, `p2-05`). 앱을 다시 켜서 같은 검색을 하면 8K 17이 선택되어 있다(`p3-01`).
      키 모드 → 8K 폴더에서 같은 묶음을 검색해도 8K 17이다(`p4-01`). 종료 후 `beetle-app.exe` 프로세스 없음.
    - 알려진 점:
      - 상세 패널의 노트 수는 LN 규칙별 수(`notes_for`)이고 로딩 카드는 CN 기준 `notes_count`다. 같은 채보가 1,412와 1,440으로 보인다.
        U3b 이전부터 그랬고 바꾸지 않았다(U3c 정렬에서 같이 정리할 수 있다).
      - 라이브러리 검색에 `AIRSHAVER`가 나오지 않았다(`songs/AIRSHAVER.bmsp`가 있는데도 0건). 원인은 확인하지 않았다. 묶음 검증은 Baby로 했다.
      - 확인하지 못한 것: 묶음 행 칩의 호버 강조, BKSP·ESC의 묶음 행 동작(키 경로는 코드로만 확인), 게임플레이 끝나고 결과에서 돌아온 뒤 선택 유지.
- **U3c — 정렬 · 필터 칩 · 검색** (완료)
  - [x] **정렬 메뉴**: 상단바 `정렬` 값을 클릭하면 드롭다운이 열린다. 다섯 정렬(제목·레벨·클리어·정확도·BPM)을 `SortMode::ALL`
        순서로 보이고, 쓰고 있는 항목은 청록 막대로 표시한다. 항목을 클릭하면 고르고, 바깥을 클릭하거나 ESC를 누르면 닫는다.
        F2 순환은 그대로다. 메뉴 열기는 클릭만 되고 키보드 열기는 없다(정렬을 바꾸는 키는 F2).
        오름·내림 토글은 넣지 않았다. `sort_songs`에 방향 인자가 없고, 뒤집으면 동점 정렬인 제목 순서까지 뒤집히므로 따로 한다.
        묶음 행은 U3b의 위치 규칙을 그대로 따른다.
  - [x] **필터 모델** (`filters.rs`, 순수 함수, 테스트): `Filter { modes, level_min, level_max, only_unplayed, only_uncleared }`.
        모드는 집합이고 비면 전체다. 레벨은 양 끝 포함이다. 미플레이는 기록이 없는 곡, 미클리어는 기록이 없거나 최고 기록이 실패(`Failed`)인 곡이다.
        레벨 칸은 라이브러리에 있는 레벨만 한 칸씩 움직이고, 최소는 왼쪽 끝 `최소`(제한 없음), 최대는 오른쪽 끝 `최대`에서 멈춘다.
        최소와 최대가 엇갈리는 이동은 하지 않는다.
  - [x] **적용 범위**: `build_tree`, `entries_for`(검색 풀), `result_count`가 같은 통과 마스크(`pass_mask`)를 쓴다.
        그래서 리프 목록, 폴더 개수, 검색 결과가 서로 맞는다. 필터로 빈 폴더는 목록에서 빠진다.
        묶음은 통과한 채보만 묶고, 기억한 채보가 걸러지면 통과한 첫 채보를 보인다.
  - [x] **필터 줄 자리**: 상단바 아래 y 70–94(1280×720 단위)에 둔다. 목록은 y 100부터 시작하고(예전 88), 9행은 그대로다.
        칩은 모드(라이브러리에 있는 모드만), 레벨 `‹ 값 ›` 두 개(최소·최대), 미플레이만, 미클리어만, 초기화(필터가 켜졌을 때만)다.
        켜진 칩은 `CYAN` 토큰으로 채운다. 오른쪽 끝에 `N곡 찾음`을 두고, 검색어가 있거나 필터가 켜졌을 때만 보인다.
  - [x] **키**: `F10`으로 필터 줄에 포커스를 준다(`F6`은 전역 게이지 키라 피했다). ←/→ 이동, ENTER 실행(레벨 칸은 한 칸 올림),
        ↑/↓ 레벨 칸 올림·내림, ESC나 F10은 포커스를 놓는다. 그 밖의 키는 포커스를 놓고 평소처럼 처리한다.
        정렬 메뉴가 열려 있으면 ↑/↓(J/K)로 고르고, ENTER·SPACE로 고르며, ESC로 닫는다.
  - [x] **빈 결과**: 필터 때문에 곡이 없으면 `필터에 맞는 곡이 없습니다.`와 `필터를 바꾸거나 초기화하세요.`, 그리고 `초기화` 버튼을 보인다.
        검색어 때문이면 기존 문구를 그대로 쓴다.
  - [x] **설정 저장** (`config.dat`): `filter_modes=4k,6k`(모드 id, 비면 전체), `filter_level_min=`·`filter_level_max=`(0 = 제한 없음),
        `filter_unplayed=0|1`, `filter_uncleared=0|1`. 기본값은 필터 없음이고, 옛 설정 파일에는 줄이 없어도 그대로 읽힌다.
  - [x] **문자열**: `strings.rs`의 `FILTER`·`FILTER_LEVEL`·`FILTER_LEVEL_MIN_ANY`·`FILTER_LEVEL_MAX_ANY`·`FILTER_UNPLAYED`·`FILTER_UNCLEARED`·
        `FILTER_RESET`·`FILTER_NO_MATCH`·`FILTER_EMPTY_HINT`·`RESULT_COUNT`. 키캡 `F10`은 영문 그대로다.
  - [x] **행 정보 정리**(미플레이 행의 `기록 없음` 대신 흐린 램프 + 빈칸): U3d에서 했다.
  - 결과 (2026-10-10):
    - 테스트: `filters.rs` 9개(기본값, 모드 집합, 레벨 범위, 미플레이·미클리어, 레벨 단계, 최소 칸, 최대 칸, 엇갈림 금지, 라이브러리에 없는 레벨).
      `folders.rs` 2개 추가(통과 마스크가 트리·검색·결과 수에 반영됨, 모드 목록과 `mode_from_id`). `config.rs` 1개(필터 왕복과 기본값).
      `cargo test --workspace`, `cargo test -p beetle-render --release --tests`, `cargo fmt --all -- --check` 통과.
    - 렌더 캡처 5장(`d3d11_select.rs`의 `song_select_filter_and_sort_views`, `scratch/u3c/captures/`):
      `select-sort-menu`(정렬 메뉴), `select-filter-chips`(모드 7K, 레벨 12 이상, 미플레이만), `select-filter-focus`(7K 칩에 포커스),
      `select-search-count`(검색 `a`, `6곡 찾음`), `select-empty-filter`(필터로 빈 목록, `초기화`). 1280×720에서 잘리거나 겹친 곳은 없다.
      처음에는 필터 줄이 상단바 구분선(y 64)을 가로질러 글자가 지워져 보였다. 그래서 y 70으로 내렸다.
    - 실제 앱(격리 `scratch/ux-before/run`, PostMessageW 클릭·키, PrintWindow 캡처, 실제 커서 불변):
      - 정렬 값 클릭 → 메뉴(`scratch/u3c/live/b1-sort-open.png`), `클리어` 클릭 → 정렬 값이 바뀌고 메뉴가 닫힘(`b2`).
      - 메뉴가 열린 상태에서 바깥 클릭 → 닫힘, 정렬 그대로(`d1-outside-click-closed.png`). ESC → 닫힘(`d2-esc-closed.png`).
      - 미플레이만 클릭(`b4`). 7K 칩 클릭 → 폴더 개수가 23곡으로 줄고 `23곡 찾음`, 초기화 칩이 나타남(`b5-unplayed-7k.png`).
      - 앱 재시작 → 필터(7K·미플레이만)와 정렬(클리어)이 그대로(`c1-restart-kept.png`). 초기화 클릭 → 216곡으로 돌아옴(`c2-reset-click.png`).
      - F10 → ENTER(4K), F10 → → → ENTER(6K) → 140곡 찾음(`c4-key-4k-6k.png`). 종료 후 `beetle-app.exe` 프로세스 없음.
    - 확인하지 못한 것: 정렬 메뉴의 ↑/↓/ENTER 키, 필터 줄의 ←/↑/↓ 키, 빈 목록의 `초기화` 버튼 클릭은 실제 앱에서 보지 않았다(코드 경로만).
      마우스로 폴더 행을 두 번 눌러 여는 것은 이번 실행에서 열리지 않았다(첫 클릭으로 선택만 됨). 원인은 확인하지 않았고 ENTER로 대신 열었다.
      U3a·U3b에서 본 클릭 타이밍 문제와 같은 종류인지도 확인하지 못했다.
    - 크기: `beetle-app.exe` 3,109,376 B(U3b 기록 3,090,432 B) → +18,944 B, +0.61%. 기준은 U3b 기록이고 HEAD를 다시 빌드하지 않았다.
- **U3d — 푸터 · 도움말 · 빈 서재 · 끌어다 놓기 · 행 정보 · 램프 분포** (완료)
  - [x] **긴 목록에서 행 클릭** (커밋 `f2db50f`, `fix(app,render): rows stay under the pointer when clicking a long song list`):
        목록의 창 위치(`AppState::list_scroll`)를 선택과 분리했다. 키보드 이동은 커서를 가운데에 둔다(`centre_list`).
        클릭은 창 위치를 바꾸지 않는다. 휠은 창 위치만 옮기고(`scroll_list`), 커서는 창을 벗어날 때만 따라간다.
        창 위치 계산은 `window_start`, `scroll_by`, `clamp_into_window`(순수 함수, 단위 테스트).
        재현: 60줄 목록에서 행을 누르면 목록이 가운데로 다시 맞춰져, 같은 자리를 다시 누르면 다른 행이 열렸다.
        테스트 `clicking_a_row_in_a_long_list_keeps_it_under_the_pointer`가 이 순서를 확인한다.
  - [x] **푸터 3~5개**: `↑↓ 이동`, `ENTER 플레이`(폴더 줄에서는 `열기`), `BKSP 뒤로`(루트가 아닐 때만), `TAB 옵션`, `? 도움말`.
        나머지 키(`/` 검색, `F1 F3` 폴더, `F2` 정렬, `F4` 설정, `A` 자동, `F12` 키 설정, `ESC` 종료)는 도움말에만 있다.
        푸터 키캡은 모두 버튼이다.
  - [x] **도움말 키**: `?`(Shift+/)와 `H`. `/`는 검색, `F1`·`F3`는 폴더 키라 피했다. 열기는 `?`·`H`·푸터 `?` 키캡,
        닫기는 `?`·`H`·ESC·패널 바깥 클릭(`Blocker`). 도움말이 열린 동안 키와 휠은 목록에 가지 않는다.
        내용은 여섯 묶음(이동, 폴더, 곡, 옵션과 설정, 필터와 정렬, 기타)이고, 키 줄과 마우스 줄(`마우스`, `끌어다 놓기` 표시)을 함께 적었다.
        표는 `select.rs`의 `HELP_GROUPS`이고, 키와 마우스 동작이 같은 함수를 부르는 것은 기존 규칙 그대로다.
  - [x] **빈 서재 안내**: 곡이 데모뿐이면(`folders::is_demo`만 남음) 목록 자리에 안내를 그린다(`first_run_guide`).
        1) `곡 관리자 열기`: `bpm-gui.exe`를 실행 파일 옆에서 찾아 별도 프로세스로 띄운다(기다리지 않음). 없으면 토스트.
        2) 끌어다 놓기: 설명만 있다(버튼 없음).
        3) `songs 폴더 열기`: 작업 폴더의 `songs`를 없으면 만들고 `explorer.exe`로 연다(별도 프로세스). 실패하면 토스트.
        4) `곡을 넣은 뒤 다시 읽습니다` + `다시 읽기 F5`: F5와 같은 함수(`rescan`).
  - [x] **끌어다 놓기 오버레이**: `WindowEvent::HoveredFile`이 `drop_hover`를 켜고, `HoveredFileCancelled`와 `DroppedFile`이 끈다.
        선곡 화면에서만 그린다(`draw_drop_overlay`, 클릭 영역 없음). `DroppedFile`의 처리(`open_dropped_file`)는 그대로다.
  - [x] **행 정보**: 기록 없는 행은 오른쪽에 아무것도 그리지 않는다. 왼쪽 램프 띠는 원래 흐리게 그려진다(U3c까지 남았던 `기록 없음` 글자 제거).
  - [x] **폴더 상세의 클리어 램프 분포**: `folders::Folder::lamps`를 트리 완성 뒤 한 번 채운다(`fill_lamps`).
        분기는 고유 곡 수로 세므로 `count`와 맞는다(데모는 5K·7K 양쪽에 있어도 한 번). 상세 패널에 누적 막대와 일곱 줄(램프 색은 `theme::clear_lamp`,
        기록 없음은 흐리게)을 그린다. 테스트 `lamp_breakdown_counts_each_lamp_and_sums_branches`.
  - 결정(목록 밖):
    - 스크롤: 키 이동은 가운데, 클릭은 창 위치 불변, 휠은 창 위치만 옮기고 커서는 창을 벗어날 때만 따라간다. 휠 한 칸 = 한 줄(기존과 같음).
    - 도움말 키는 `?`(H는 보조). 푸터는 흐름에 필요한 다섯 개만 둔다.
  - 검증 (2026-10-10):
    - `cargo test --workspace`, `cargo test -p beetle-render --release --tests`, `cargo fmt --all -- --check` 통과.
      새 테스트: `select.rs`(창 위치, 휠, 클릭 회귀, 도움말·안내·끌어다 놓기 한 묶음, 도움말 표의 키·캡션), `folders.rs`(램프 분포와 분기 합), `handlers/song_select.rs`(bpm-gui 위치, `?`·H 규칙).
      글자 범위 테스트 통과(새 문자열 전부 KR 폰트).
    - 렌더 캡처(`target/select-*.bmp` → `scratch/u3d/*.png`): `select-help`, `select-drop`, `select-guide`, `select-folder-lamps`, `select-noplay`(미플레이 행).
      읽어서 확인했다. `select-folder-root`는 픽스처 폴더의 램프가 0이라 분포가 그려지지 않는다(앱에서는 합이 곡 수와 같다).
      도움말이 전체 목록 위에 뜨면 드로우콜이 두 개가 된다(아래 한계).
    - 실제 앱(`scratch/ux-before/run`, PostMessageW·PrintWindow, 실제 커서 불변. 클릭은 이동 한 칸 옮긴 뒤 누름, 실행 전 `folder_path`를 루트로 초기화.
      결과 캡처는 `scratch/u3d/live-final/`):
      - 전체 곡에서 여섯째 행 클릭 → 선택만 되고 목록이 그대로(`a1-row-selected.png`). 같은 자리를 다시 클릭 → 그 행의 로딩 화면(`a2`: ABYSSAL // COSMOS 6K 23).
        같은 실험을 수정 전 커밋(`84ed892`)으로 하면 두 번째 클릭이 다른 행(ALL NIGHT)을 골라 버그가 재현된다.
      - 폴더 행 두 번 클릭 → 키 모드 열림, 빵부스러기 `전체 > 키 모드`, 램프 분포 `4K` 16곡 = 1+1+14(`b1`).
      - `?` 열림(`b2`), ESC 닫힘(`b3`), 패널 바깥 클릭 닫힘(`b4`), 푸터 `?` 클릭 열림(`b5`).
      - 휠 3칸 아래: 창이 3줄 내려가고, 선택은 맨 위 줄로 옮겨졌다. 목록은 가운데로 다시 맞추지 않는다(`c2`).
      - 빈 서재(`scratch/u3d/empty-run`, 곡 없음, 표 없음): 안내 화면(`e1`). `곡 관리자 열기` 클릭 → `bpm-gui.exe` 실행 → WM_CLOSE로 종료(`e2`).
        `다시 읽기` 클릭 → "곡 1개를 읽었습니다" 토스트(`e4`, 데모만 집계된 값).
      - 램프 분포 합: 전체 곡 216 = 4 + 1 + 5 + 206. 키 모드 216 = 4 + 1 + 5 + 206(`k1`). 수정 전 빌드에서는 키 모드가 217로 나왔다.
        데모 곡이 5K와 7K에 같이 있어 자식 합을 쓰면 한 번 더 셌기 때문이다. 고유 곡 수로 바꿔 고쳤다.
      - 실행 뒤 `beetle-app.exe`와 `bpm-gui.exe` 프로세스 0개.
    - 확인하지 못한 것:
      - 끌어다 놓기 오버레이는 실제 앱에서 띄우지 못했다(PostMessage로 파일 드롭을 만들 수 없다). 캡처와 이벤트 연결 코드로만 확인했다.
      - `songs 폴더 열기`는 탐색기 창을 띄우지 않으려고 누르지 않았다. 경로 생성과 `explorer.exe` 실행은 코드로만 확인했다.
      - 빈 서재에서 `곡 관리자`를 연 뒤 창 안의 동작은 보지 않았다(창이 뜨고 닫히는 것만 확인).
  - 한계 (알려진 점):
    - 도움말이 열린 채 전체 곡 목록을 그리면 정점 수가 백엔드의 배치 한도(8192)를 넘어 드로우콜이 두 개가 된다(`select-help`는 두 개, 나머지 선곡 화면은 한 개).
      도움말은 잠깐 여는 상태라 두었다. 화면 전체 한 드로우콜 원칙의 예외다.
    - 푸터에서 빠진 `A`(자동), `F12`(키 설정), `ESC`(종료) 키캡은 마우스로 바로 누를 수 없다. 키는 그대로이고 도움말에 적혀 있다.
      상단 바에 놓을지는 U5·U6에서 정한다.
    - 빈 서재 안내는 데모만 있을 때 뜬다. 데모가 아닌 곡이 하나라도 있으면 목록이 나온다.
  - 크기: `beetle-app.exe` 3,109,376 B(U3c 기록, `84ed892`를 다시 빌드해 확인) → 3,109,888 B(커밋 1) → 3,179,008 B(U3d 완료).
    U3d로 +69,632 B(+2.2%). 도움말 표와 오버레이, 안내 문자열, 램프 분포 그림이 들어갔다. 크기 원인은 따로 나누어 재지 않았다.

### U4 — 게임플레이 HUD (완료)
U4는 두 커밋이다. 먼저 크기 원인을 줄이고(커밋 `22a66af`), 그다음 HUD를 바꾼다.

- **크기 정리 (커밋 `22a66af`, `perf(app): open bpm-gui and the songs folder with ShellExecuteW instead of std::process`)**
  - U3d 뒤 `beetle-app.exe`가 +69 KB 늘었는데, 원인은 `std::process::Command`였다. 선곡의 `bpm-gui.exe` 실행과
    `explorer.exe <songs>` 실행을 `handlers/song_select.rs`의 `shell_open`(`ShellExecuteW`, shell32 직접 FFI,
    동사 `open` / `explore`)으로 바꿨다. 성공 판정은 반환값 32 초과, 오류 토스트는 그대로다.
  - 결과: `beetle-app.exe` 3,179,008 B → 3,122,176 B (−56,832 B). 20 KB 기준을 넘어 추가 조사는 하지 않았다.
    U3d의 나머지 약 12 KB(도움말 표·오버레이·안내 문자열·램프 분포)는 그대로 둔다.
- [x] **BGA 없을 때 레이아웃** (진단 14): 곡에 BGA 이벤트도 영상도 없으면(`has_bga`, 옵션이 꺼져 있어도 같다) 빈 BGA
      상자 대신 **판정 타임라인**을 그린다. 왼쪽·오른쪽 플레이필드는 점수 패널을 키우고(236 px, EX 점수 52 pt,
      판정 범례 간격 넓힘) 그 아래 타임라인과 스펙트럼을 둔다. 가운데 플레이필드는 BGA 상자가 없으면 정보 열이
      왼쪽 전체(곡 정보·점수·타임라인)를 맡고 오른쪽에는 게이지만 남긴다. 스테이지 이미지가 있으면 어둡게(`BG` 170)
      타임라인 뒤에 깐다. 타임라인은 최근 8 s(오디오 시계 기준)의 판정을 점으로 찍는다. 시간은 오른쪽에서 왼쪽으로
      흐르고, 세로 위치는 판정 차이(위 = FAST, 아래 = SLOW, ±150 ms가 가장자리)다.
      주의: 키 안내 줄을 두 줄로 감쌀 때 스펙트럼과 겹쳐 예약 높이를 26 → 36 px로 늘렸다(BGA 상자가 조금 작아짐).
- [x] **플레이 중 키 안내** (진단 15): 설정 · 레이아웃의 **플레이 중 키 안내** 행(`KeyHintSetting`,
      `config.dat`의 `play_key_hint=first|always|off`, 기본 `first`). "처음 3초만"은 오디오 시계로 3 s까지 완전히 보이고
      0.5 s에 걸쳐 사라진다(`KeyHintSetting::alpha`, 단위 테스트). "항상"·"끄기"는 그대로다. 리플레이·자동 플레이에서는
      같은 줄(`ESC 곡 선택으로`)이 같은 규칙을 따른다.
- [x] **READY** (진단 16 일부): 곡 시작부터 첫 노트 1 s 전까지 레인 위쪽 1/5 자리에 `READY`와
      `F3/F4 그린 넘버 / F10/F11 레인 커버`를 띄운다. 첫 노트 1.5 s 전 페이드는 0.5 s에 걸쳐 끝나며(`ready_alpha`),
      첫 노트가 1.5 s보다 빠르면 READY를 그리지 않는다. 첫 노트는 지뢰·롱노트 끝을 뺀 노트 중 가장 이른 시각이다.
      표시만 바꾸고 판정 시각은 건드리지 않는다.
- [x] **종료 배너** (진단 16 일부): 곡이 끝나면(또는 게이지 실패) 기록은 한 번 저장하고(`finish_gameplay`, 이미 끝난
      상태면 아무것도 하지 않음), 플레이필드 가운데에 **클리어 램프 이름**(`ClearType::as_str`: `FULL COMBO`, `PERFECT`,
      `HARD CLEAR`, `CLEAR`, `EASY CLEAR`, `FAILED`)을 1.5 s 띄운 뒤 결과로 간다. 배너 동안 콤보·판정 글자는 숨긴다.
      ENTER·ESC는 결과로 바로 넘어간다(`leave_gameplay`). 자동 플레이·리플레이도 같은 배너를 띄운다.
      **예외(INV-1)**: 배너 타이머만 `Instant`(벽시계)를 쓴다. 배너 동안에는 판정도 노트 이동도 없고, 실패한 곡은 오디오가
      이미 멈춰 있어서 오디오 클럭이 의미 없다. 판정과 노트 위치는 여전히 `AudioClock`만 쓴다.
      저장은 `finish_gameplay`에서만 하며 배너 중에 다시 하지 않는다. ESC가 일시정지가 아니라 결과 건너뛰기가 되는 것도
      배너 동안뿐이다.
- [x] **그린·커버 변경 표시** (진단 17): F3/F4/PageUp/PageDown/1/2로 그린이 바뀌면 `그린 1470 ms`, F10/F11로 커버가
      바뀌면 `커버 35%`를 레인 위쪽에 1 s 보인다(마지막 0.3 s 페이드, 오디오 시계 기준, `gameplay_readout`).
- 문자열: `strings.rs`에 `READY`, `READY_HINT`, `READOUT_GREEN`, `READOUT_COVER`, `TIMELINE_TITLE`,
  `TIMELINE_RANGE`, `TIMING_FAST`, `TIMING_SLOW`, 설정 행 `ROW_KEY_HINT`, `HELP_KEY_HINT`, `KEY_HINT_FIRST/ALWAYS/OFF`.
  `READY`는 영문 그대로 두었다(화면에 떠야 하는 게임 용어, 요청 문구). 글자 범위 테스트 통과.
- 구조: `JudgeMark`(`view.rs`, 최근 8 s 판정 기록, 메인 스레드에서만 쓴다), `PlayFrame`에 `has_bga`,
  `key_hint_alpha`, `readout`, `banner`, `judge_marks` 추가. 게임플레이 프레임은 여전히 한 드로우콜이다
  (`whole_frame_is_one_batch_without_bga`에 배너·readout·타임라인을 함께 켠 프레임 추가).
- 결과 (2026-10-10):
  - 크기: `beetle-app.exe` 3,122,176 B(커밋 1) → 3,128,320 B (+6,144 B, +0.20%).
  - 검증: `cargo test --workspace` 통과, `cargo test -p beetle-render --release --tests` 통과, `cargo fmt --all -- --check` 통과.
    새 테스트: `play.rs`(`ready_alpha` 구간과 1.5 s 규칙, 첫 노트 계산, 한 드로우콜), `config.rs`(키 안내 알파와 순환,
    설정 왕복), 캡처 `d3d11_play.rs::gameplay_hud_variants`(왼쪽·가운데 BGA 없음, READY, 그린·커버 readout, FULL COMBO·FAILED
    배너, 키 안내 끔). 모든 캡처 프레임은 드로우콜 1개다.
  - 렌더 캡처(`scratch/u4/`, 1280x720, PNG 전부 확인): `play-7k-nobga-left-timeline`, `play-7k-nobga-center`,
    `play-7k-ready`, `play-7k-readout-green`, `play-7k-readout-cover`, `play-7k-banner-fullcombo`,
    `play-7k-banner-failed`, `play-14k`(키 안내가 스펙트럼과 겹치던 문제를 고친 뒤). 기존 `play-7k`, `play-paused`도 다시 확인.
  - 실제 앱(격리 `scratch/ux-before/run`, `PostMessageW` 키·클릭, `PrintWindow` 캡처, 실제 커서 불변. `scratch/u4/live/`):
    - 자동 플레이 `Beetle Demo Track`(7K, 36 노트, 150 BPM): 시작 0 s에 키 안내 줄이 보이고(`seq/f000`), 1.8 s에도 보이며,
      2.8 s 무렵에는 사라졌다(`seq/f005`, `seq/f008`).
    - F3 → `그린 1470 ms`(`seq/f028`, 9.8 s). F10 → `커버 35%`(`seq/f032`, 11.1 s).
    - 끝 배너 `PERFECT`(72/72 전부 PGREAT, `seq/f044`), 약 1.5 s 뒤 결과 화면(`seq/f048`, 자동 플레이라 `기록이 저장되지 않음`).
    - 배너 중 ENTER: 결과가 바로 나왔다(`seq3/f046`, 15.2 s). 같은 실행에서 배너 없이 두면 약 16 s에 나온다.
    - 설정: F4 → 플레이 중 키 안내 행 → `항상`(`key-hint-always.png`). `config.dat`에 `play_key_hint=always`가 저장됐고,
      다시 `처음만`으로 되돌려 `play_key_hint=first`로 저장됐다.
    - 종료 후 `beetle-app.exe` 프로세스 없음.
  - FPS: **측정하지 못했다.** (→ U5에서 측정, 아래 "U5 측정") devtools의 fps 줄은 캡처 모드(`BEETLE_CAPTURE`)에서만 찍히는데, 이번 세션의 게임플레이 캡처
    실행이 게임플레이에 들어가지 않았다(곡 선택 상태 문제로 추정, 확인하지 않음). WARP도 같은 이유로 재지 못했다.
  - 확인하지 못한 것:
    - READY의 실제 화면: 데모 곡의 첫 노트가 1.5 s보다 빨라서 READY가 나오지 않는다. 화면은 캡처(`play-7k-ready`)와 단위 테스트로만 확인.
    - 키 안내의 부분 페이드(2.5–3.5 s 사이 프레임)는 실제 앱 프레임으로 잡지 못했다(알파 계산은 단위 테스트).
    - FULL COMBO·FAILED 배너는 실제 앱에서 보지 못했다(캡처만). 실제 앱에서는 PERFECT만 확인.
    - 일반 수동 플레이의 기록 저장 1회(배너 중 ESC·ENTER 포함): 자동 플레이로만 확인했다. 저장 경로는 `finish_gameplay`의
      가드로 한 번만 가도록 했고 단위 테스트는 없다.
    - 설정 행 클릭은 실제 앱에서 누르지 않았다(키로만 확인).
    - 배너 중 ESC는 실제 앱에서 누르지 않았다(ENTER와 같은 경로).

### U5 — 결과 (완료)
- [x] **게이지 추이 그래프** (진단 18): `beetle_core::GaugeTrend`(`crates/beetle-core/src/judge/gauge_trend.rs`).
  곡 시작(`finalize_start_gameplay`)에 곡 길이 기준으로 버퍼를 한 번 잡는다(`with_capacity`, 최대 512점,
  간격 = 곡 길이 / 511, 최소 0.05 s). `tick_gameplay`가 **오디오 시계 시각**으로 게이지를 찍는다(INV-1).
  실패 시각은 간격과 무관하게 그 순간을 기록하고, 실패 뒤에는 찍지 않는다. 그리기는 한 칸이 약 3 px인
  계단형 선(얇은 사각형, 최대 160칸), 아래 그라데이션 면, 클리어 기준선(EASY·GROOVE 80%, HARD·HAZARD는
  실패선 0%, 점선), 실패 지점의 빨간 세로선과 `FAILED`로 이루어진다. 색은 기존 `gauge_color`.
  위치: 왼쪽 패널에서 태그 아래, 최종 게이지 바 위(기존 게이지 바는 그대로 둠). 곡선이 면을 가지므로 한 배치
  (드로우콜 1) 한도 안에서 칸 수를 줄였다(조밀한 추이 테스트 포함).
- [x] **다음 행동**: 푸터 4개 — 곡 선택(ENTER), 다시 하기(R), **옵션 바꿔 다시 하기**(TAB), 스크린샷(P).
  TAB은 선곡의 플레이 옵션 패널(`options_table::PLAY_OPTIONS`, `draw_options_panel`의 `OptionsFooter::Retry`)을
  결과 위에 연다. 패널 아래 **새 옵션으로 다시 하기** 버튼(ENTER 또는 클릭)은 옵션을 저장하고 같은 곡을
  다시 시작한다(게이지·모디파이어 등은 새 플레이를 시작할 때 읽힌다). TAB·ESC는 저장만 하고 닫는다.
  결과 화면의 패널에서 ENTER는 값 바꾸기가 아니라 시작이다(Space는 값을 한 칸 앞으로). 선곡의 패널은 그대로다.
- [x] **마우스**: 푸터 4개, 패널의 행·‹ ›·시작 버튼, 패널 바깥 클릭(닫기)을 `HitId`로 처리한다
  (`handlers/mouse.rs`의 `result_click`). 키와 같은 함수(`open_options`, `retry_with_options`, `retry_song`)를 부른다.
- [x] **신기록 표기**(U1d 남은 문제 해결): 점수·통계 옆 태그는 `신기록`(EX 점수, 최대 콤보, MISS 수),
  램프가 좋아졌을 때는 `램프 갱신`, 배너(클리어 판정 아래)는 `신기록`(`NEW_RECORD`). 태그는 모두 9 px로
  같은 크기다(이전 8 px).
- [x] **저장 한 번**(U4 잔여): `gameplay::PlayEndGuard`가 플레이의 끝을 한 번만 받아들인다. `finish_gameplay`가
  `take()`로 확인하고, 배너 중이거나 배너를 ENTER로 건너뛴 뒤 들어온 끝 신호는 무시한다. 새 플레이에서
  `reset`한다. 테스트: 끝 → 건너뛰기 → 다시 끝 → 저장 1회.
- 결과 (2026-10-10):
  - 크기: `beetle-app.exe` 3,128,320 B(HEAD `6ee9239`를 같은 조건으로 다시 빌드해 잰 값) → 3,132,928 B
    (+4,608 B, +0.15%). 새 크레이트 없음.
  - 검증: `cargo test --workspace`, `cargo test -p beetle-render --release --tests` 통과, `cargo fmt --all -- --check` 통과.
    새 테스트: `gauge_trend.rs`(점 수 상한, 시각 단조 증가, 간격, 되돌아간 시각 무시, 실패 지점 한 번만,
    상한에서의 실패, 샘플링 중 재할당 없음), `gameplay.rs`(PlayEndGuard 2개), `stage_result.rs`(조밀한 추이를
    포함한 프레임이 한 배치), `d3d11_result.rs`(GROOVE 클리어 + 추이, HARD 실패 + 마커, HARD 클리어, 자동 플레이,
    옵션 패널 위 — 모두 드로우콜 1).
  - 렌더 캡처(`scratch/u5/`, 1280×720, PNG 다섯 장 모두 확인): `result-clear.png`(추이·램프 갱신·신기록),
    `result-failed.png`(실패 마커, 실패선), `result-hard-clear.png`(HARD 클리어, 바닥 실패선),
    `result-auto.png`(자동 플레이, 평평한 100%), `result-options.png`(옵션 패널 위, 새 옵션으로 다시 하기 버튼).
  - 실제 앱(`scratch/ux-before/run`, `PostMessageW` 키·클릭, `PrintWindow` 캡처, 실제 커서 불변, 자동 플레이
    `Beetle Demo Track`, 게이지 설정을 HARD에서 시작): 결과 추이(`scratch/u5/live/03-result-auto.png`),
    TAB → 게이지 HARD → GROOVE → 패널(`05-…`), ENTER → GROOVE로 다시 시작(`06-…`: 3 s 뒤 게이지 바 61%.
    HARD였다면 100%), 두 번째 결과의 추이가 20%에서 올라 80% 기준선을 넘음(`07-result-2.png`),
    R로 다시 시작(`08-…`), ENTER → 선곡(`09-…`, 게이지 칩 GROOVE).
    **마우스만**: 결과 TAB 버튼 클릭 → 패널(`21-mouse-tab-options.png`) → 시작 버튼 클릭 → 재시작(`22-…`) →
    결과 ENTER 버튼 클릭 → 선곡(`24-mouse-back-select.png`). 종료 후 `beetle-app.exe` 없음.
  - 한계: 실패 결과와 HARD 실패 중 건너뛰기는 실제 앱으로 보지 않았다(캡처와 단위 테스트). 수동 플레이의
    저장은 자동 플레이로만 확인했고, 저장 한 번은 `PlayEndGuard` 테스트로 확인했다. 패널에서 행 클릭·‹ › 클릭은
    실제 앱에서 누르지 않았다(키와 같은 함수).
  - 커밋 분리: 코어(`gauge_trend`), 앱·렌더 기능(가드 포함), 문서. 가드 테스트는 `gameplay.rs`의 다른 변경과
    한 커밋에 있다(같은 파일의 조각을 나누지 않으려고).
- U4 FPS 잔여: 아래 **U5 측정** 참고(P4 표 옆에도 기록).

#### U5 측정: 게임플레이 FPS (U4 잔여 5)
- 원인: devtools의 `BEETLE_CAPTURE_SCREEN=gameplay`는 게임플레이에 들어간 뒤에야 캡처·fps를 찍는다. 앱은
  곡을 스스로 시작하지 않으므로, 선곡에서 ENTER를 보내야 한다. U4의 실행은 이 ENTER가 들어가지 않았다(추정).
  이번에 확인한 점: 검색창이 열린 상태의 ENTER는 검색을 확정할 뿐 곡을 시작하지 않는다(두 번째 ENTER에 시작).
  또 `Boot`(곡 목록 읽기)가 끝나기 전 ENTER도 버려진다. 실행 스크립트는 `scratch/u5/fps.ps1`.
- 조건: 1280×720, 창 모드, 릴리스, 프레임 제한 해제(`target_fps=0`), 자동 플레이, 10 s 측정.
  BGA 곡은 `虹のわすれもの`(7K 2, 184노트, 영상 BGA), BGA 없는 곡은 `Beetle Demo Track`(7K, 36노트).

| 조건 | HEAD (`6ee9239`, U4) | U5 (이번 변경) |
|---|---|---|
| 하드웨어, 7K BGA 곡 (영상 BGA 켬) | 1,868 / 1,618 fps | 2,138 / 1,864 fps |
| 하드웨어, 같은 곡 BGA 끔 | — | 2,582 fps |
| 하드웨어, 7K BGA 없는 곡 | 2,543 / 2,587 fps | 2,917 / 2,574 fps |
| WARP, 7K BGA 곡 | 126 fps | 129 fps |
| WARP, 7K BGA 없는 곡 | 166 fps | 169 fps |

- P4 표(하드웨어 2,772 / WARP 162, MilK 자동 플레이)와 비교하면 BGA 없는 곡은 같고(2,574–2,917), WARP도
  같다(129–169). BGA 곡의 하드웨어 값은 P4보다 23–33% 낮지만 **U4(HEAD)에서도 같은 값**이고, BGA를 끄면
  2,582 fps가 나와 원인은 이 곡의 영상 BGA 디코드·합성 비용이다(P4 곡의 BGA와 다르다). U5 변경과 U4
  HUD 변경이 원인은 아니다. 30%를 넘는 회귀로 보지 않아 고치지 않았다.
- 판정 타임라인(U4)은 BGA 없는 곡에서 그려지는데, 그 곡들의 값이 HEAD와 같아 그쪽 비용도 두드러지지 않는다.

### U6 — 마무리 (완료, 2026-10-10)
- [x] **UI 배율 옵션: 뺀다** (감독 결정). 메뉴는 창 높이로 이미 배율이 바뀌고(`viewport.scale` = 높이/720) 레이아웃이
      1280×720 단위로 짜여 있다. 배율을 더 키우면 720p에서 넘친다. 실제 문제는 글자가 작은 것이므로 아래 1번으로 해결했다.
- [x] **1. 최소 글자 크기** (`80fb825`, `fix(render): raise the minimum text size on every screen`): 캡션·라벨 12 px 이상,
      태그·칩·키캡 11 px 이상(720 단위, `* s` 전). 9·10 px `caption(…)`은 12 px로, 9 px 태그는 `TextStyle::new(11.0 * s)`로 바꿨다.
      `screens/mod.rs`의 `no_screen_draws_text_below_the_floor`가 리터럴 크기를 검사한다(계산된 `size * s`는 값이 적힌 곳에서 본다).
      바뀐 뒤 생긴 겹침 두 곳을 고쳤다: 결과의 `신기록` 태그가 `최대 콤보` 캡션을 가려서 라벨 옆으로 옮겼고, 게임플레이 판정
      타임라인 제목이 패널 테두리에 붙어서 베이스라인을 4 px 내렸다. 선곡·키 설정·설정·도움말·로딩 캡처를 확인했다.
- [x] **2. 토스트 자리** (`9f9089c`, `fix(app,render): toasts no longer cover controls`): `ToastAnchor`.
      선곡·결과·설정은 **푸터 왼쪽**(x 140 단위, 푸터 높이만큼 채움: 푸터 왼쪽은 선곡의 곡 수 말고는 비어 있다).
      키 설정은 **모드 탭 아래 띠**(y 152 단위, 탭과 안내 줄 아래, 건반 위). 이전에는 선곡 필터 줄과 키 설정 모드 탭을 덮었다.
      실제 앱 확인: 키 설정 F1 프리셋 토스트가 탭 아래에 뜨고 건반을 가리지 않음(`scratch/u6/live/keys-toast.png`),
      선곡 F5 재스캔 토스트가 푸터 왼쪽에 뜸(`scratch/u6/captures/select-toast.png`).
- [x] **3. 곡 수** (`04b54f8`, `fix(render): song select footer counts charts like the search result`): **단위는 채보(곡)다.**
      푸터 `N / M 곡`: N = 보이는 목록의 채보 수(묶음 행은 묶인 채보 수, 폴더 행은 폴더 안 곡 수), M = 라이브러리 전체 채보.
      이미 "N곡 찾음"이 채보 수였으므로 같은 단위다. 폴더만 있는 목록은 `N 폴더`를 그대로 둔다.
      전체 곡 216 / 필터 7K·미플레이만 23(`N / 216 곡`), 검색 결과 0은 `0 / 216 곡`. 단위 테스트 `footer_count_is_in_charts`.
      "전체 M곡 + 행 N" 방식은 쓰지 않았다(행 기준 전체 수는 라이브러리 전체를 다시 묶어야 해서 계산이 두 벌이 된다).
- [x] **4. 판정 색 (색각)**: **옵션을 넣지 않았다**(결론: 이미 구별된다). 방법: `theme.rs`의 토큰 값을 Viénot·Brettel 계열
      선형 RGB 행렬(protan·deutan)로 바꾸고 CIE76 ΔE를 쟀다(`scratch/u6/cvd.py`, 근사치). 인접 판정 최소 ΔE:
      | 쌍 | 정상 | 적색색약 | 녹색색약 |
      |---|---|---|---|
      | PGREAT–GREAT | 42 | 29 | 23 |
      | GREAT–GOOD | 98 | 19 | 31 |
      | GOOD–BAD | 159 | 114 | 94 |
      | BAD–POOR | 82 | 66 | 87 |
      | POOR–MISS | 82 | 19 | 39 |
      | FAST–SLOW | 106 | 75 | 91 |
      가장 가까운 쌍도 ΔE 19 이상이고, 판정은 색 옆에 **항상 이름이 함께** 나온다(플레이 점수 패널의 범례, 판정 텍스트,
      결과의 판정 행). 색만 있는 곳은 타임라인 점뿐이고 같은 화면 범례로 읽힌다. 색각 사용자가 직접 보고 확인하지는 않았다.
- [x] **추가 요청: 그린 넘버 값 표기** (`a1bd74c`, `fix(app,render): show the green number without the 그린 word`):
      옵션 칩은 `1470`, 게임플레이 readout은 `1470 ms`. 라벨(`그린 넘버`, 키 안내 `1/2 그린 넘버`, `READY` 안내)은 그대로다.
      `CHIP_GREEN`·`READOUT_GREEN`만 바꿨고, 테스트 픽스처와 주석도 맞췄다. 선곡 칩은 라벨 없이 숫자만 남아, 옆의 `REGULAR`·`HARD`와
      섞여 읽힌다는 점을 확인해 달라(캡처 `scratch/ux-after/songselect.png`).
- [x] **5. 측정** (`scratch/u6/fps/results.txt`, 1280×720 창, 프레임 제한 해제(`target_fps=0`), 릴리스, 10 s 창).
      | 화면 | 하드웨어 | WARP | 비고 |
      |---|---|---|---|
      | 부팅 | 60 | 측정 안 함 | 캐시 없는 전체 스캔 0.5 s, 부팅 화면은 60 Hz로 돈다 |
      | 선곡 | 60 | 60 | 메뉴는 vsync 60 상한(`about_to_wait`: 게임플레이 외 vsync) |
      | 설정 | 60 | 60 | 위와 같음 |
      | 키 설정 (7K) | 60 | 60 | 위와 같음 |
      | 로딩 | 85 | 74 | 창이 0.1 s뿐이라(9·8 프레임) 값이 의미 없음 |
      | 게임플레이, BGA 있음 (`Love & Justice`, 자동) | 2,081 | 143 | 영상 BGA |
      | 게임플레이, BGA 없음 (`Beetle Demo Track`, 자동) | 2,829 | 181 | 판정 타임라인 |
      | 결과 | 60 | 60 | vsync 상한 |
      - 메뉴 60 fps는 렌더 비용이 아니라 vsync 상한이다. 메뉴 렌더 비용은 이 방법으로 재지 못한다.
      - U5의 BGA 곡(`虹のわすれもの`)과 이번 BGA 곡은 다르다(한글·일본어 검색은 IME가 켜진 검색창에서 키 입력으로 들어가지 않아
        `Love`로 찾았다). 값을 U5와 직접 비교하지 말 것.
      - 측정 빌드는 `a1bd74c` 이전(문자열만 다른 차이)이다.
      - 크기(릴리스, HEAD `48b560d`): `beetle-app.exe` 3,012,096 B(U0) → **3,132,416 B** (+120,320 B, +4.0%).
        `bpm-gui.exe` 4,832,256 B, `bpm.exe` 2,974,208 B(U6에서 처음 기록; 이전 기준 없음).
        `beetle-app.exe`는 AGENTS.md의 목표(개별 실행 파일당 < 1 MB)를 이미 넘겼다. 이 차이는 U1–U5 기능 추가 몫이다.
- [x] **6. AGENTS.md §4** (`48b560d`, `docs(agents): module notes for the render and app UI pieces`): `beetle-render`에 `screens/`·문자열 표·
      `hit.rs`·`theme.rs` 토큰 규칙을, `beetle-app`에 폴더 트리·필터·곡 묶음, `handlers/`(키·마우스 공용 함수), `ime.rs`,
      `calibration.rs`, `transition.rs`를 한 줄씩 추가했다. 불변식·의존성 정책은 바꾸지 않았다.
- [x] **7. 전후 캡처** (`scratch/ux-after/`, 나란히 놓은 PNG는 `scratch/ux-compare/`, 합성은 `scratch/ux-compare/compose.ps1` (System.Drawing)).
      실행 폴더는 `scratch/ux-before/run/`이고 `config.dat`는 매번 같은 복사본으로 되돌렸다. 짝 목록(왼쪽 전, 오른쪽 후):
      `boot`, `songselect`, `select-options`, `select-exit`, `loading`, `play-7k`(자동, `Beetle Demo Track`), `result`(자동), `keyconfig`.
      새 화면: `settings`, `folder-root`(왼쪽은 검은 칸, 전 버전에 없음). 주의: `folder-root`는 ESC로 만든 루트 목록이고,
      `songselect`는 저장된 `folder_path=all` 상태다.
      캡처 중 발견: `songselect` 캡처를 `folder-root` 실행 뒤에 찍으면 저장된 폴더가 루트로 남는다. 실행마다 설정을 되돌렸다.
- [x] **8. 마무리**: 플랜 상태를 완료로 표시하고 아래 **남은 일**을 적었다.

#### U6 검증
- `cargo test --workspace` 전부 통과, `cargo test -p beetle-render --release --tests` 통과, `cargo fmt --all -- --check` 통과.
- 새 테스트: `no_screen_draws_text_below_the_floor`, `footer_count_is_in_charts`. 기존 글자 범위 테스트 통과.
- 렌더 캡처(`scratch/u6/captures/`, 전부 PNG로 확인): 결과 신기록 태그 위치, 게임플레이 타임라인 제목 위치, 선곡·키 설정·설정·도움말·로딩.
- 실행 뒤 `beetle-app.exe` 프로세스 없음. 워크트리 없음. 실제 커서 불변(PostMessageW·PrintWindow만 사용).

#### 남은 일 (U6 이후)
- **검증하지 못한 것**
  - 실제 한글·일본어 IME 조합과 후보창 위치(합성 메시지에는 IME 문맥이 없다).
  - 캘리브레이션 정확도(사람이 누른 입력으로 측정한 값은 아직 없다).
  - 실제 파일 끌어다 놓기(오버레이·오류 토스트), `songs 폴더 열기`와 `곡 관리자` 창 안의 동작.
  - FULL COMBO·FAILED 배너를 실제 앱에서 본 적이 없다(캡처와 단위 테스트뿐).
  - 선곡 정렬 메뉴의 ↑↓ENTER, 필터 줄의 ←↑↓, 빈 목록의 `초기화` 클릭, 빵부스러기 클릭, 묶음 행의 BKSP·ESC.
  - 판정 색의 실제 색각 확인(시뮬레이션은 근사치).
  - 로딩 화면 FPS(창이 0.1 s라 측정 불가). 메뉴 렌더 비용(vsync 상한 때문에 재지 못함).
  - 그린 넘버 칩의 가독성(라벨 없는 숫자, 위 결정 참고).
- **미룬 것**
  - 메뉴 액션 계층과 게임패드(U1b 보류): 컨트롤러 입력을 붙일 때 기준을 다시 정한다.
  - 도움말이 열린 채 전체 목록을 그리면 드로우콜이 2개(U3d 한계, 화면 한 드로우콜 원칙의 예외).
  - 푸터에서 빠진 `A`·`F12`·`ESC` 키캡(도움말에만 있음).
  - 렌더 텍스트 크기: 태그·칩은 `TextStyle::new(11.0 * s)` 리터럴로 남아 있다(테스트가 리터럴 값을 본다).
  - `beetle-app.exe`가 1 MB 목표를 넘은 상태(3.13 MB). 줄일 방법은 따로 측정해서 정한다.
- **범위 밖 문제(U3b에서 발견)**
  - ~~라이브러리 검색에 `AIRSHAVER`가 나오지 않는다~~ → **원인 확인: (c) 의도된 동작.** `songs/AIRSHAVER.bmsp`는 설치되지 않은 패키지다. 스캐너(`scanner.rs`)는 폴더에서 `bms/bme/bml/pms`만 읽고, 패키지는 레지스트리의 활성 상태만 읽는다(942634e). `packages/`에는 `registry.json`이 없어 활성 패키지가 0개다(`bpm list`: "No packages installed."). Baby·514nm 등은 라이브러리 폴더(`D:\U_E`)의 `.bms`다. 격리 실험(`scratch/airshaver-run`, 복사본 설치)에서는 `bpm install` 뒤 `bpm songs`에 AIRSHAVER(6채보: 7K·14K)가 나왔다. 코드 변경 없음. 제안: 재스캔(F5)이나 부팅 때 `songs/`에 설치되지 않은 `.bmsp`가 있으면 토스트로 "설치되지 않은 패키지 N개: AIRSHAVER.bmsp — `bpm install`로 설치" 안내. 의미 변경은 하지 않았다.
  - 노트 수 1,412 vs 1,440: 상세 패널은 LN 규칙별 `notes_for`, 로딩 카드는 CN 기준 `notes_count`를 쓴다. 한쪽으로 맞춰야 한다.

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
