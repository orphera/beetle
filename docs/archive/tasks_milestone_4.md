# tasks_milestone_4.md — Milestone 4 작업 아카이브 (BMS Package Delta Engine)

이 문서는 **Milestone 4 (BMS Package Delta 차분 및 원자적 업데이트 엔진)**의 전체 구현 내역 및 완료된 체크리스트를 영구 보관하는 아카이브 문서입니다.

---

# 🚀 Milestone 4: BMS Package Delta(차분) 및 원자적 업데이트 엔진 (Delta & Atomic Update Engine) (Completed)

Milestone 4의 목표는 [BMS Package System 표준 명세](../specs/bms_package_system.md)와 [ADR-015](../DECISIONS.md#adr-015-bms-패키지-차분delta-시스템-및-버전-전이-모델)에 따라, **BMS 작품의 대용량 재배포를 방지하고 차분 제작자/원곡자의 배포 마찰(Friction)을 제로화하는 고성능 차분(Delta) 생성·적용·원자적 업데이트 시스템**을 구축하는 것이었습니다.

핵심 원칙:
1. **결정론적 변환**: $\text{Apply}(\text{Package@base}, \text{Delta}(\text{base} \to \text{target})) = \text{Package@target}$
2. **원자적 안전성 (Atomic Safety)**: 패치 실패 시 기존 설치본 파괴 0% (Rollback & Full Package Fallback)
3. **제작자 마찰 제로 (Zero-Friction Creator UX)**: BMS 헤더 메타데이터 자동 추출 및 1-클릭/1-명령어 `.bmdp` 생성

---

## 📋 Phase 1: `bms-package` 차분 포맷 & Diff/Patch 코어 라이브러리 (`crates/bms-package/src/delta/`) (Completed)
- [x] **차분 메타데이터 모델 정의 (`delta/manifest.rs`)**
  - [x] `DeltaManifest`: `package_id`, `base_state_hash`, `target_state_hash`, `base_checksum`, `target_checksum`
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
  - [x] 1단계: 설치된 패키지 state와 Base State 일치 확인
  - [x] 2단계: 임시 스테이징에서 Delta 적용 및 타겟 재현
  - [x] 3단계: 복원된 Target Package 무결성(SHA-256) 검증
  - [x] 4단계: 원자적 설치(`Atomic Commit`) 및 `registry.json` 버전 갱신
  - [x] 실패 시 기존 버전 100% 무손상 유지 및 롤백 보장
- [x] **Full Package Fallback 및 자동 복구 (Repair)**
  - [x] Base state 미설치/불일치 시 `BaseStateNotInstalled` 명확한 에러 전파 및 Full Package 수용 기반 마련

---

## 📋 Phase 3: `bpm` CLI 차분 명령어 및 제작자 툴링 (`crates/bpm/`) (Completed)
- [x] **`bpm diff` 서브커맨드**
  - [x] `bpm diff <base.bmsp> <target.bmsp> -o <diff.bmdp>`
  - [x] 디렉터리 기반 diff: `bpm diff <base_dir> <target_dir> -o <diff.bmdp>`
- [x] **`bpm patch` 서브커맨드**
  - [x] `bpm patch <base.bmsp> <diff.bmdp> -o <target.bmsp>`
- [x] **`bpm pack --base` 차분 제작자 지원 플래그**
  - [x] 새로 만든 채보 폴더에서 BMS 헤더(#TITLE, #ARTIST, #LEVEL) 자동 추출 후 Base 패키지와 묶어 즉시 `.bmdp` 생성
- [x] **`bpm update` 서브커맨드 연동**
  - [x] 로컬 차분 패키지를 감지하여 1-명령어 자동 원자적 업데이트

---

## 📋 Phase 4: `bpm-gui` 차분 제작 마법사 & 업데이트 UI 및 전체 회귀 검증 (`crates/bpm-gui/`) (Completed)
- [x] **`bpm-gui` 차분 제작 마법사 (Package & Delta Creator 모달/단축키)**
  - [x] Base 곡 / Target 폴더 입력 후 1-클릭 Delta(`.bmdp`) 빌드 (`[C]`/`F4`)
  - [x] 원본 곡 선택 및 차분 생성 파이프라인 연동
- [x] **1-클릭 패치 및 업데이트 UI**
  - [x] `.bmdp` 파일 드래그 앤 드롭 또는 단축키(`[D]`/`F3`) 시 자동 차분 패치 적용
  - [x] 백그라운드 Worker 스레드 + 회전 스피너 논블로킹 UI 연동
- [x] **전체 워크스페이스 회귀 검증 및 바이너리 크기 확인**
  - [x] `cargo test --workspace` (전체 69개 테스트 100% 통과)
  - [x] 바이너리 크기 < 1 MB 불변식 확인 (`beetle-app.exe: ~1.03 MB`, `bpm-gui.exe: ~1.04 MB`, `bpm.exe: ~0.60 MB`)
