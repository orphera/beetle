# TASKS.md — Beetle 로드맵 및 개발 체크리스트 (Milestone 9)

이 문서는 Beetle 프로젝트의 활성 마일스톤 구현 태스크를 관리하는 로드맵 문서입니다.

> 💡 **이전 마일스톤 완료 내역 아카이브**:
> - [archive/tasks_milestone_1.md](archive/tasks_milestone_1.md): 기반 아키텍처, 오디오 엔진, 패키지 포맷 및 1차 게임 루프
> - [archive/tasks_milestone_2.md](archive/tasks_milestone_2.md): UI/UX 전면 개편, 다국어 폰트, 인게임 일시정지, 결과 보상 화면, 1:1 키 리바인딩
> - [archive/tasks_milestone_3.md](archive/tasks_milestone_3.md): 아키텍처 모듈화 & 클린 구조 리팩토링 (`beetle-render` 및 `beetle-app` 서브모듈화)
> - [archive/tasks_milestone_4.md](archive/tasks_milestone_4.md): BMS Package Delta(차분) 및 원자적 업데이트 엔진 (`.bmdp` 빌더, 패치 및 GUI 마법사)
> - [archive/tasks_milestone_5.md](archive/tasks_milestone_5.md): 디스플레이 다원화, 종횡비 보존 렌더링, 초고주사율 최적화, BGA 엔진 및 WMF 동영상 지원
> - [archive/tasks_milestone_6.md](archive/tasks_milestone_6.md): 초경량 멀티 백엔드 GPU 하드웨어 가속 렌더링 엔진, CJK 한자 폴백, 진성 GPU 배치 파이프라인
> - [archive/tasks_milestone_7.md](archive/tasks_milestone_7.md): 듀얼 아틀라스(Sound & BGA Atlas) 초고속 패키지 엔진, 무손실 FLAC 압축, WebDAV VFS 온더플라이 WAV 합성
> - [archive/tasks_milestone_8.md](archive/tasks_milestone_8.md): 원격 패키지 레지스트리 및 온라인 송 허브 (`bpm update/search/install/upgrade`, `bpm serve`, `bpm-gui` 온라인 탭)

---

# 🚀 Milestone 9: 순수 .bmsp 아카이브 전용 저장소 (Pure BMSP Archive Storage)

자세한 기술 설계 및 아키텍처는 [specs/pure_bmsp_storage.md](specs/pure_bmsp_storage.md)를 참조합니다.

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
- [ ] **`crates/bms-package-manager/src/lib.rs` 단위 테스트 최신화**
  - [ ] 디스크에 풀린 개별 파일(`video.mp4`, `song.bms`, `01.wav`) assert를 `open().contains(...)` 및 `package.bmsp` 존재 확인으로 갱신
- [ ] **`vfs.rs`, `export.rs`, `serve.rs`, `updater.rs` 동작 확인**
  - [ ] `package.bmsp` 기반 동작이 깨지지 않고 100% 정상 작동하는지 확인

---

## 📋 Phase 4: 전체 워크스페이스 검증 및 릴리스 빌드
- [ ] `cargo check --workspace` 타입 체크
- [ ] `cargo test --workspace` 전체 테스트 통과 검증
- [ ] `cargo build --release` 바이너리 크기 및 정상 동작 확인

---

## 🔭 향후 확장 제안 및 백로그 (Future Proposals & Backlog)
- [proposals/legacy_compatibility_vfs.md](proposals/legacy_compatibility_vfs.md): 레거시 구동기(LR2/beatoraja) 하위 호환을 위한 무설치 WebDAV VFS 마운트 및 FUSE 확장 제안서.
- [proposals/platform_expansion.md](proposals/platform_expansion.md): Linux 네이티브 데스크톱 지원, WebAssembly(WASM/Web Audio) 무설치 웹 플레이어/뷰어, 모바일/태블릿 터치 제스처 지원 제안서.
- [proposals/decoupled_bga_package_system.md](proposals/decoupled_bga_package_system.md): 오디오/차트 코어 패키지와 대용량 BGA 패키지의 완전 분리 및 온디맨드 결합 제안서.
- [proposals/gameplay_enhancement_and_display.md](proposals/gameplay_enhancement_and_display.md): 플레이 옵션(스피드, 판정 오프셋) 및 디스플레이 고도화 제안서.
