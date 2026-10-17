---
name: Ship UX polish
overview: U1–U6에서 화면과 입력 모델은 이미 갖춰져 있으므로, 이번엔 비주얼 재설계가 아니라 첫 10분 흐름(곡 넣기 → 키 확인 → 플레이)과 bpm-gui의 남은 거친 점을 고쳐 배포 가능한 제품 완성도를 맞춘다.
todos:
  - id: p0-autofocus-rescan
    content: 선곡 창 포커스 복귀 시 songs/registry mtime으로 필요할 때만 재스캔 + 관리자 열기 토스트
    status: completed
  - id: p0-first-run-copy
    content: 빈 서재 안내를 주 경로(관리자) + 보조로 줄이고, 첫 플레이 전 키 설정 한 번 안내
    status: completed
  - id: p0-audio-fail
    content: AudioEngine 생성 실패 시 무음 플레이 대신 선곡 복귀 + 오류 토스트
    status: completed
  - id: p1-header-actions
    content: 선곡 상단바에 자동/키설정/종료 버튼, 그린 칩 접두, 도움말 열림 시 드로우콜 1개 유지
    status: completed
  - id: p2-bpm-dpi-ime
    content: bpm-gui scale_factor 레이아웃, 검색 IME, 빈 바로설치/추가 완료 후 Beetle 복귀 안내
    status: completed
  - id: p3-copy-pause
    content: 부팅 경과시간 제거, 로딩 취소 힌트, 일시정지 마우스 정합
    status: completed
  - id: verify
    content: 캡처·빈서재 워크스루·DPI 확인·릴리스 크기 기록
    status: pending
isProject: false
---

# 배포용 UI/UX 폴리싱 (beetle-app + bpm-gui)

U1–U6([docs/plans/2026-10-10-beetle-app-ux-redesign.md](docs/plans/2026-10-10-beetle-app-ux-redesign.md))과 bpm-gui 개편([docs/plans/2026-10-10-bpm-gui-ux-redesign.md](docs/plans/2026-10-10-bpm-gui-ux-redesign.md))으로 **화면·마우스·IME·폴더 트리·옵션 분리**는 끝난 상태다. 남은 갭은 “처음 켠 사람이 설명서 없이 곡을 넣고 칠 수 있는가”와 “두 앱이 같이 배포됐을 때 흐름이 끊기지 않는가”다.

설치 프로그램·포터블 zip·게임패드는 이번 범위 밖이다. 디자인 시스템 재작성, 타이틀 화면 추가, UI 배율 옵션도 하지 않는다(U6 결정).

```mermaid
flowchart LR
  empty[빈서재] --> manager[bpm-gui로곡추가]
  manager --> focus[Beetle창복귀]
  focus --> rescan[자동다시읽기]
  rescan --> keys[키확인안내]
  keys --> play[첫플레이]
```

## 원칙

- AGENTS.md 불변식 유지. 새 크레이트 없음. 색은 `theme.rs` 토큰만.
- 화면마다 커밋 + 렌더 캡처 + 실제 앱에서 해당 흐름 한 번.
- 바이너리 크기는 단계마다 측정. `std::process` 재도입 금지(U4에서 `ShellExecuteW`로 줄인 이유).

## P0 — 첫 세션이 끊기지 않게 (최우선)

지금 빈 서재 안내([crates/beetle-render/src/screens/select.rs](crates/beetle-render/src/screens/select.rs) `first_run_guide`)는 버튼을 주지만, 곡 관리자를 연 뒤에는 **F5를 직접 눌러야** 목록이 생긴다([crates/beetle-app/src/handlers/song_select.rs](crates/beetle-app/src/handlers/song_select.rs) `open_song_manager`는 fire-and-forget).

- **창이 다시 포커스를 받으면 서재가 바뀌었는지 보고 다시 읽기.** `notify` 크레이트 없이 `songs/`와 `packages/registry.json`의 mtime만 비교. 관리자를 연 뒤 복귀, Explorer에서 파일을 넣은 뒤 복귀, 끌어다 놓기 직후를 같은 경로로 처리. 변화가 없으면 스캔하지 않음.
- **관리자를 연 직후 토스트**를 “곡을 넣은 뒤 이 창으로 돌아오면 목록이 갱신됩니다”로 바꿔, F5 지식을 요구하지 않음. F5는 유지.
- **빈 서재 안내를 한 경로로.** 4단계(관리자 / 끌어다 놓기 / 폴더 / 다시 읽기)는 초보에게 선택지가 많다. 주 버튼은 `곡 관리자 열기`, 보조는 끌어다 놓기 한 줄 + `songs 폴더` 링크로 줄인다. 다시 읽기는 자동 갱신이 있으면 안내에서 빼도 된다.
- **첫 플레이 전 키 확인.** 커스텀 바인딩이 한 번도 없으면 선곡에서 한 번만 배너/토스트(`F12 키 설정` 또는 상단 버튼). 첫 ENTER를 가로채 키 설정으로 보내지 않음(방해가 됨).
- **오디오 장치가 없으면 조용히 진행하지 않기.** [crates/beetle-app/src/gameplay.rs](crates/beetle-app/src/gameplay.rs)의 `AudioEngine::new(soundbank).ok()`는 실패 시 무음 플레이가 된다. 선곡으로 되돌리고 오류 토스트(`오디오 장치를 찾지 못했습니다`).

## P1 — 숨은 동작을 화면에서 보이게

U3d에서 푸터를 줄인 뒤 `A` / `F12` / `ESC`는 도움말(`?`)에만 있다.

- 선곡 **상단바**에 아이콘/텍스트 버튼: 자동 플레이 토글, 키 설정, 종료. 푸터는 지금처럼 흐름 키만. [crates/beetle-render/src/screens/select.rs](crates/beetle-render/src/screens/select.rs) 상단바와 [hit.rs](crates/beetle-render/src/hit.rs) `HitId`.
- **그린 넘버 칩**에 짧은 접두(예: `G 1470`)를 다시 붙여, `HARD` 칩과 구분(U6에서 라벨을 뺀 잔여).
- 도움말이 열린 동안 **목록을 그리지 않거나** 단순화해 배치 한도(8192) 초과로 드로우콜이 2개가 되는 예외를 없앤다.

## P2 — bpm-gui를 같이 쓸 수 있게

남은 공식 이슈: 고DPI에서 글자가 작고, 검색에 한글 IME가 없다.

- **DPI:** [crates/bpm-gui/src/widgets.rs](crates/bpm-gui/src/widgets.rs)가 물리 픽셀 고정(`PX_TITLE = 18`). `winit` `scale_factor`를 레이아웃 단위에 곱한다. 스냅샷 테스트는 배율 1.0을 기준으로 유지하고, 배율 1.5 한 장을 추가로 확인.
- **검색 IME:** beetle-app의 [ime.rs](crates/beetle-app/src/ime.rs) 패턴을 bpm-gui 검색칸에만 복제(검색 포커스일 때만 `set_ime_allowed`). 새 공유 크레이트는 만들지 않음.
- **바로 설치 탭:** 기본 서버가 비어 있으면 “아직 받을 수 있는 목록이 없습니다. 난이도표나 곡 추가로 넣으세요” 빈 상태를 분명히. 탭을 숨기지는 않음(기존 결정).
- 곡 추가가 끝나면 상태줄에 **Beetle 창으로 돌아가면 목록이 갱신된다**는 한 줄(P0과 같은 계약).

## P3 — 카피·미세 연출 (화면 재구성 없이)

- **부팅:** 경과 시간 `0.0s`는 개발자용이다. 사용자에게는 상태 문구만([boot.rs](crates/beetle-render/src/screens/boot.rs)).
- **로딩:** 진행률 채널을 새로 파지 않는다(로더는 완료만 보낸다). 상태 문구만 읽히게 다듬고, 취소(ESC → 선곡)가 있으면 힌트에 적는다.
- **일시정지:** 클릭/호버가 선곡 모달과 같게([play.rs](crates/beetle-render/src/screens/play.rs) `pause_menu` + `handlers/mouse.rs`).
- 문자열은 전부 [strings.rs](crates/beetle-render/src/strings.rs) / bpm-gui 문구 상수. KR 폰트 글자 범위 테스트 유지.

## 하지 않는 것

- 타이틀 화면, UI 배율 옵션, 게임패드 액션 계층, 설치 프로그램.
- bpm-gui를 D3D 스킨으로 다시 그리기(렌더러가 다름).
- 로딩 결정적 프로그레스 바(워커 프로토콜 변경은 이번 폴리싱 대비 비용이 큼).

## 검증

- `cargo test --workspace`, `cargo test -p beetle-render --release --tests`, bpm-gui 스냅샷.
- 실제 앱 워크스루: 빈 폴더 → 관리자로 `.bmsp` 설치 → Beetle 포커스 → 목록 등장 → 키 안내 → 플레이. 오디오 장치 없음 토스트. 125%/150% DPI에서 bpm-gui 가독성.
- 릴리스 크기 기록(`beetle-app.exe`, `bpm-gui.exe`).