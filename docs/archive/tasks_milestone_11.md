# tasks_milestone_11.md — Milestone 11 완료 아카이브

이 문서는 Milestone 11 (게임 안에서 곡 넣기)의 완료된 세부 태스크 아카이브입니다.

---

## Milestone 11: 게임 안에서 곡 넣기 (In-Game Song Install)

설계와 확정된 결정은 [plans/2026-10-21-song-install-in-app.md](../plans/2026-10-21-song-install-in-app.md)를 참조합니다.

- [x] **S1** 설치 진입점(`bpm-gui` `tasks::add_path`)을 `bms-package-manager::add`로 옮기고 `AddReport`를 반환하게 하기 (폴더 연결 `FolderMode::Link`, 프로필 선택 포함)
- [x] **S2** 게임 창에 끌어다 놓기: 압축 파일은 Turbo `.bmsp`로 묶어 설치, 폴더는 참조로 연결, 요약 토스트 (`install.rs`, 실행 파일 +607 KB)
  - 2026-10-22 사용자 확인: 실행 파일 +607 KB 증가 승인
- [x] **S3** 첫 실행 안내를 "기존 BMS 폴더 연결 / 끌어다 놓기" 두 갈래로 줄이고, 설정 화면 "곡 폴더" 목록(추가·연결 끊기)을 둠 (`song_folders.rs`, 폴더 선택 대화상자는 `bms-package-manager::file_dialog`로 옮김)
  - 2026-10-22 사용자 확인: 실제 동작 정상
- [x] **S4** 선곡 화면 곡 메뉴(우클릭 / M): 폴더 열기, 패키지 곡은 삭제(확인)·버전 고르기(2개 이상일 때), 폴더 곡은 목록에서 숨기기(`hidden.dat`, 설정 "숨긴 곡"에서 모두 다시 보이기) (`song_menu.rs`)
  - [ ] 패키지 곡의 메뉴·삭제·버전 바꾸기는 아직 확인하지 않음(쓸 일이 생길 때 확인, 캡처는 폴더 곡 메뉴만). 미리 듣기가 같은 `.bmsp`를 열고 있으면 삭제가 실패할 수 있음(오류 토스트)
- [x] **S5** `.bmsp` 파일 연결(설정 ".bmsp 파일 열기", HKCU) + 단일 인스턴스 넘겨주기(이름 있는 뮤텍스 + 메시지 전용 창 `WM_COPYDATA`) (`single_instance.rs`, `file_assoc.rs`)
  - 2026-10-22 사용자 확인: 파일 연결, 탐색기 더블클릭, 넘겨주기 정상
- [x] **S6** `bpm-gui`를 "고급 도구"로 자리 정리: 창 제목·머리말·사용법을 "고급 도구"로 바꾸고, 게임 설정 시스템 묶음에 "고급 도구 / 열기" 행을 둠(`shell.rs`). 목록·버튼 행(곡 폴더, 숨긴 곡, 고급 도구)은 좌우 화살표를 그리지 않음(`OptionLine::stepper`)
  - 2026-10-22 사용자 확인: 설정에서 고급 도구 실행 정상
