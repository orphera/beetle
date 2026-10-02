# tasks_milestone_9.md — Milestone 9 완료 아카이브

이 문서는 Milestone 9 (순수 .bmsp 아카이브 전용 저장소)의 완료된 세부 태스크 아카이브입니다.

---

# 🚀 Milestone 9: 순수 .bmsp 아카이브 전용 저장소 (Pure BMSP Archive Storage)

자세한 기술 설계 및 아키텍처는 [specs/pure_bmsp_storage.md](../specs/pure_bmsp_storage.md)를 참조합니다.

---

## 📋 Phase 1: `bms-package-manager::storage` 설치 파이프라인 다이어트 (`crates/bms-package-manager/src/storage.rs`)
- [x] **`install_package_with_progress()` 리팩토링**
  - [x] 루즈 파일 추출 루프(`entries` 순회 `fs::write`) 완전 제거
  - [x] 임시 폴더(`.tmp_install/`)에 `manifest.json`과 `package.bmsp`만 원자적으로 저장 및 커밋
- [x] **`install_companion()` & `remove_companion()` 리팩토링**
  - [x] 비디오 파일 디스크 추출 및 탐색 삭제 로직 제거
  - [x] `<id>.bga.bmsp` 아카이브 파일만 해당 state 디렉터리에 단일 파일로 저장 및 원자적 삭제

---

## 📋 Phase 2: `beetle-app::scanner` 패키지 탐색 최적화 (`crates/beetle-app/src/scanner.rs`)
- [x] **채보 없는 BGA 컴패니언 아카이브 건너뛰기**
  - [x] `packages/` 탐색 시 `*.bga.bmsp` 파일은 불필요하게 열지 않고 조기 스킵
  - [x] `package.bmsp` 단일 파일 인식 및 `virtual_path` 인덱싱 무결성 확인

---

## 📋 Phase 3: 단위 테스트 갱신 및 VFS / Exporter 정합성 확인
- [x] **`crates/bms-package-manager/src/lib.rs` 단위 테스트 최신화**
  - [x] 디스크에 풀린 개별 파일(`video.mp4`, `song.bms`, `01.wav`) assert를 `open().contains(...)` 및 `package.bmsp` 존재 확인으로 갱신
- [x] **`vfs.rs`, `export.rs`, `serve.rs`, `updater.rs` 동작 확인**
  - [x] `package.bmsp` 기반 동작이 깨지지 않고 100% 정상 작동하는지 확인

---

## 📋 Phase 4: 전체 워크스페이스 검증 및 릴리스 빌드
- [x] `cargo check --workspace` 타입 체크
- [x] `cargo test --workspace` 전체 테스트 통과 검증
- [x] `cargo build --release` 바이너리 크기 및 정상 동작 확인

---

## 🎨 부록: UI 디자인 시스템 수립 및 화면 전면 적용 (Milestone 9 종료 후 추가 작업)
Milestone 9 완료 직후, 룩앤필 전수 리뷰를 통해 발견된 UI 문제를 수정하고 `docs/specs/ui_design_system.md`를 신설했습니다.
- [x] 곡선택 하단 상태바 텍스트 겹침 수정 (`render_song_select` 2줄 푸터로 분리)
- [x] 게임플레이 BGA/HUD 패널 폭 데드스페이스 제거 (고정 520px 캡 삭제, `max_w` 전체 사용)
- [x] 난이도 LV 배지 색상 범례 추가 (`level_tier_label()`, 디테일 패널 LEVEL 박스)
- [x] PLAY OPTIONS 모달 섹션 그룹핑 (PLAY / AUDIO / DISPLAY-SYSTEM / INPUT & SESSION)
- [x] `docs/specs/ui_design_system.md` 신설 (타이포그래피 4단계, 색상 의미 체계, 테두리 정책, 그룹핑 컨벤션)
- [x] 테두리 다이어트 전면 적용: Song Select, Result, Gameplay HUD(비주얼라이저), Pause 모달, Key Config
- [x] Exit 확인 모달 / Loading 화면 검토 완료 (기능적 테두리이므로 변경 없음으로 확정)
