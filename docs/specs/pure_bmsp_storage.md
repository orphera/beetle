# 순수 .bmsp 아카이브 전용 저장소 규격서 (Pure BMSP Archive Storage Specification)

본 문서는 `bms-package-manager`의 패키지 설치 파이프라인에서 불필요한 개별 파일 추출(Loose Files Extraction)을 완전히 배제하고, **순수 단일 아카이브(`package.bmsp`) 및 최소 메타데이터(`manifest.json`)만을 보관**하는 순수 아카이브 전용 저장소(Pure BMSP Storage) 아키텍처에 대한 공식 기술 규격서입니다.

---

## 1. 아키텍처 배경 및 동기 (Motivation)

### 1.1 초기 구현(Milestone 2)의 잔재 및 문제점
Beetle 프로젝트 초기(Milestone 2)에는 `beetle-app`에 `.bmsp` 직접 로더가 없었고, VFS 마운트 도구도 존재하지 않았습니다. 이로 인해 패키지 설치 시 `packages/<id>/<state>/` 폴더에 모든 음원(WAV), 이미지(BMP), 채보(BMS) 파일을 일일이 압축 해제하고, 원본 `package.bmsp`까지 함께 이중 저장하는 과도기적 방식을 채택했습니다.

이로 인해 다음과 같은 심각한 비효율이 발생했습니다:
1. **저장 공간 낭비**: 압축 파일과 압축 해제된 파일이 공존하여 1곡당 디스크 사용량이 2배로 증가.
2. **파일시스템 오버헤드**: BMS 1곡당 1,000~3,000개의 자잘한 파일이 생성되어, 수백 곡 설치 시 수십만 개의 파일로 인해 Inode 고갈, 디스크 슬랙 스페이스(Slack Space) 낭비, 안티바이러스 실시간 감시 부하 유발.
3. **설치/삭제 속도 저하**: 수천 회의 개별 파일 I/O 및 디렉터리 생성으로 인해 설치와 삭제에 불필요한 수 초의 대기 시간 소요.

### 1.2 생태계 기술 완성에 따른 전환 당위성
현재 Beetle 생태계는 다음과 같은 핵심 기술들이 모두 완성되어 있습니다:
* **`beetle-app` 인-아카이브 스트리밍 로더 (`loader.rs`)**: `.bmsp` 내부에서 직접 채보를 파싱하고 메모리로 PCM/이미지/비디오를 즉시 디코딩.
* **`bpm mount` WebDAV VFS (`vfs.rs`)**: 레거시 구동기(LR2, beatoraja) 호환을 위해 `.bmsp` 내부를 가상 WAV/BMP 파일로 실시간 노출.
* **`bpm export` (`export.rs`)**: 채보 수정이나 개별 파일 확인이 필요할 때 언제든 1초 만에 일반 폴더로 역추출.

따라서 디스크에 개별 파일들을 미리 풀어둘 이유가 완전히 소멸되었으며, **단일 아카이브 보관 모드로 전환하는 것이 Beetle의 경량화 철학에 100% 부합**합니다.

---

## 2. 저장소 디렉터리 레이아웃 변경 (Storage Layout)

### 변경 전 (Legacy Hybrid Layout)
```text
packages/
└── <package_id>/
    └── <state_hash>/
        ├── manifest.json
        ├── package.bmsp                    <-- 원본 아카이브
        ├── <package_id>.bga.bmsp           <-- 동반 BGA 아카이브 (선택)
        ├── song_hyper.bms                  <-- [중복] 수천 개의 풀린 파일들
        ├── song_another.bms
        ├── 01.wav ~ 99.wav
        ├── bga_001.bmp ~ bga_999.bmp
        └── bga.mp4
```

### 변경 후 (Pure Archive Layout)
```text
packages/
├── registry.json                           <-- 전체 패키지 상태 추적 레지스트리
└── <package_id>/
    └── <state_hash>/
        ├── manifest.json                   <-- 빠른 메타데이터 조회를 위한 1KB 텍스트
        ├── package.bmsp                    <-- 단일 무손실 결정론적 아카이브
        └── <package_id>.bga.bmsp           <-- 동반 BGA 아카이브 (분리 설치 시에만 존재)
```
* **곡당 파일 개수**: **1,000~3,000개 ➡️ 단 2개 (`package.bmsp`, `manifest.json`)**
* **설치 속도**: 수천 회 파일 쓰기 ➡️ 단 1회 원자적 파일 복사/이동 (약 50배 단축)
* **디스크 사용량**: 약 50% 절감 (중복 제거)

---

## 3. 세부 컴포넌트 사양 및 변경점

### 3.1 `bms-package-manager::storage` (`crates/bms-package-manager/src/storage.rs`)
* **`install_package_with_progress()`**:
  * `entries`를 순회하며 디스크에 파일을 쓰던 추출 루프 제거.
  * `manifest.json`과 `package.bmsp`만 임시 폴더(`.tmp_install/`)에 쓰고 원자적 `rename`으로 커밋.
* **`install_companion()`**:
  * 비디오 파일을 디스크로 추출하던 루프 제거.
  * `<package_id>.bga.bmsp` 아카이브 파일만 해당 state 디렉터리에 복사.
* **`remove_companion()`**:
  * 디스크 상의 비디오 파일 탐색 삭제 로직 제거.
  * `<package_id>.bga.bmsp` 아카이브 파일만 원자적 삭제.

### 3.2 `beetle-app::scanner` (`crates/beetle-app/src/scanner.rs`)
* `packages/` 폴더를 재귀 탐색할 때, 채보가 들어있지 않은 BGA 컴패니언 아카이브(`*.bga.bmsp`)는 건너뛰도록 필터 추가 (`if filename.ends_with(".bga.bmsp") { continue; }`).
* `package.bmsp`를 스트리밍 스캔하여 `packages/<id>/<state>/package.bmsp::<chart_path>` 가상 경로로 인덱싱.

### 3.3 `beetle-app::loader` (`crates/beetle-app/src/loader.rs`)
* 기존의 `split_once("::")` 기반 인-아카이브 스트리밍 로더, Sound Atlas, BGA Atlas, VideoSource(Memory), 동반 BGA 자동 탐색 파이프라인을 그대로 100% 활용 (추가 변경 불필요).

### 3.4 레거시 호환 및 관리 도구 (`vfs.rs`, `export.rs`, `serve.rs`, `updater.rs`)
* `vfs.rs` (`bpm mount`), `export.rs` (`bpm export`), `serve.rs` (`bpm serve`), `updater.rs` (`bpm update` / `diff`) 모두 이미 `state_dir.join("package.bmsp")`를 기본으로 참조하도록 구현되어 있으므로 100% 호환성 유지.

---

## 4. 단계별 구현 및 검증 로드맵

1. **Step 1**: `crates/bms-package-manager/src/storage.rs` 리팩토링 (추출 루프 제거 및 순수 아카이브 저장).
2. **Step 2**: `crates/beetle-app/src/scanner.rs` 탐색 최적화 (`.bga.bmsp` 스킵).
3. **Step 3**: `crates/bms-package-manager/src/lib.rs` 단위 테스트 갱신 (풀린 파일 assert 제거 및 아카이브 무결성 검증).
4. **Step 4**: 전체 워크스페이스 검증 (`cargo test --workspace`) 및 릴리스 빌드.
