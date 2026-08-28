# TASKS.md — Beetle 로드맵 및 개발 체크리스트 (Milestone 4)

이 문서는 Beetle 프로젝트의 활성 마일스톤 구현 태스크를 관리하는 로드맵 문서입니다.

> 💡 **이전 마일스톤 완료 내역**:
> - [archive/tasks_milestone_1.md](file:///C:/Users/jeongwoong/dev/beetle/docs/archive/tasks_milestone_1.md): 기반 아키텍처, 오디오 엔진, 패키지 포맷 및 1차 게임 루프
> - [archive/tasks_milestone_2.md](file:///C:/Users/jeongwoong/dev/beetle/docs/archive/tasks_milestone_2.md): UI/UX 전면 개편, 다국어 폰트, 인게임 일시정지, 결과 보상 화면, 1:1 키 리바인딩
> - [archive/tasks_milestone_3.md](file:///C:/Users/jeongwoong/dev/beetle/docs/archive/tasks_milestone_3.md): 아키텍처 모듈화 & 클린 구조 리팩토링 (`beetle-render` 및 `beetle-app` 서브모듈화)

---

# 🚀 Milestone 4: BMS Package Delta(차분) 및 원자적 업데이트 엔진 (Delta & Atomic Update Engine)

Milestone 4의 목표는 [BMS Package System 표준 명세](file:///C:/Users/jeongwoong/dev/beetle/docs/specs/bms_package_system.md)와 [ADR-015](file:///C:/Users/jeongwoong/dev/beetle/docs/DECISIONS.md#adr-015-bms-패키지-차분delta-시스템-및-버전-전이-모델)에 따라, **BMS 작품의 대용량 재배포를 방지하고 차분 제작자/원곡자의 배포 마찰(Friction)을 제로화하는 고성능 차분(Delta) 생성·적용·원자적 업데이트 시스템**을 구축하는 것입니다.

핵심 원칙:
1. **결정론적 변환**: $\text{Apply}(\text{Package@base}, \text{Delta}(\text{base} \to \text{target})) = \text{Package@target}$
2. **원자적 안전성 (Atomic Safety)**: 패치 실패 시 기존 설치본 파괴 0% (Rollback & Full Package Fallback)
3. **제작자 마찰 제로 (Zero-Friction Creator UX)**: BMS 헤더 메타데이터 자동 추출 및 1-클릭/1-명령어 `.bmdp` 생성

---

## 📋 Phase 1: `bms-package` 차분 포맷 & Diff/Patch 코어 라이브러리 (`crates/bms-package/src/delta/`) (Completed)
- [x] **차분 메타데이터 모델 정의 (`delta/manifest.rs`)**
  - [x] `DeltaManifest`: `package_id`, `base_version`, `target_version`, `base_checksum`, `target_checksum`
  - [x] 엔트리 연산 분류: `added_resources`, `modified_resources`, `removed_resources`, `unchanged_resources`
  - [x] 매니페스트 직렬화/역직렬화 및 정규화
- [x] **결정론적 차분 빌더 (`delta/builder.rs`)**
  - [x] `DeltaBuilder`: Base 패키지와 Target 패키지 간의 리소스/차트 diff 추출
  - [x] 변경/추가된 리소스만 압축하여 `.bmdp` 아카이브 생성 (`INV-6` 결정론적 타임스탬프 및 사전순 정렬)
- [x] **차분 적용 및 타겟 재현 엔진 (`delta/applicator.rs`)**
  - [x] `DeltaApplicator`: `Base Package + Delta Archive` 검증 및 `Target Package` 완전 복원
  - [x] Base SHA-256 검증 및 생성된 Target SHA-256 일치 검증
- [x] **단위 테스트 스위트 작성**
  - [x] 차트만 추가된 케이스 (15 KB 초경량 차분)
  - [x] 키음 WAV 수정/추가/삭제 케이스
  - [x] Base 버전 불일치 및 손상된 Delta 거부 테스트

---

## 📋 Phase 2: `bms-package-manager` 원자적(Atomic) 업데이트 & 복구 엔진 (`crates/bms-package-manager/src/updater/`) (Completed)
- [x] **원자적 업데이트 파이프라인 (`updater.rs`)**
  - [x] 1단계: 설치된 패키지 버전과 Base Version 일치 확인
  - [x] 2단계: 임시 스테이징에서 Delta 적용 및 타겟 재현
  - [x] 3단계: 복원된 Target Package 무결성(SHA-256) 검증
  - [x] 4단계: 원자적 설치(`Atomic Commit`) 및 `registry.json` 버전 갱신
  - [x] 실패 시 기존 버전 100% 무손상 유지 및 롤백 보장
- [x] **Full Package Fallback 및 자동 복구 (Repair)**
  - [x] Base 버전 미설치/불일치 시 `BaseVersionNotInstalled` 명확한 에러 전파 및 Full Package 수용 기반 마련

---

## 📋 Phase 3: `bpm` CLI 차분 명령어 및 제작자 툴링 (`crates/bpm/`)
- [ ] **`bpm diff` 서브커맨드**
  - [ ] `bpm diff <base.bmsp> <target.bmsp> -o <diff.bmdp>`
  - [ ] 디렉터리 기반 diff: `bpm diff <base_dir> <target_dir> -o <diff.bmdp>`
- [ ] **`bpm patch` 서브커맨드**
  - [ ] `bpm patch <base.bmsp> <diff.bmdp> -o <target.bmsp>`
- [ ] **`bpm pack --base` 차분 제작자 지원 플래그**
  - [ ] 새로 만든 채보 폴더에서 BMS 헤더(#TITLE, #ARTIST, #LEVEL) 자동 추출 후 Base 패키지와 묶어 즉시 `.bmdp` 생성
- [ ] **`bpm update` 서브커맨드 연동**
  - [ ] 로컬 또는 원격 차분 패키지를 감지하여 1-명령어 자동 업데이트

---

## 📋 Phase 4: `bpm-gui` 차분 제작 마법사 & 업데이트 UI 및 전체 회귀 검증 (`crates/bpm-gui/`)
- [ ] **`bpm-gui` 차분 제작 마법사 (Package & Delta Creator 탭)**
  - [ ] Base 곡 선택 (라이브러리 클릭 또는 `.bmsp` 드롭)
  - [ ] 추가/수정할 `.bms` 채보 파일 드롭
  - [ ] [Export Delta (.bmdp)] 1-클릭 내보내기 버튼
- [ ] **1-클릭 패치 및 업데이트 UI**
  - [ ] `.bmdp` 파일 드래그 앤 드롭 시 자동 패치 적용
  - [ ] 업데이트 프로그레스 바 표시
- [ ] **전체 워크스페이스 회귀 검증 및 바이너리 크기 확인**
  - [ ] `cargo test --workspace` (전체 테스트 100% 통과)
  - [ ] 바이너리 크기 < 1 MB 불변식 확인 (`beetle-app.exe`, `bpm-gui.exe`, `bpm.exe`)

---

## 🔭 향후 확장 제안 및 백로그 (Future Proposals & Backlog)
- [proposals/platform_expansion.md](file:///C:/Users/jeongwoong/dev/beetle/docs/proposals/platform_expansion.md): Linux 네이티브 데스크톱 지원, WebAssembly(WASM/Web Audio) 무설치 웹 플레이어/뷰어, 모바일/태블릿 터치 제스처 지원 제안서.
- [proposals/remote_package_registry.md](file:///C:/Users/jeongwoong/dev/beetle/docs/proposals/remote_package_registry.md): 원격 패키지 레지스트리, 1-클릭 다운로드/업데이트, 정적 CDN 호스팅, LAN P2P 공유 제안서.
- [proposals/legacy_compatibility_vfs.md](file:///C:/Users/jeongwoong/dev/beetle/docs/proposals/legacy_compatibility_vfs.md): 레거시 구동기(LR2/beatoraja) 하위 호환을 위한 무설치 WebDAV VFS 마운트 및 FUSE 확장 제안서.
