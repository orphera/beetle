# BMS Package Manager Specification (`bms-package-manager`)

## 1. 목적

`bms-package-manager`는 `.bmsp` 패키지를 **설치하고 관리하며, Beetle이 설치된 BMS 콘텐츠를 발견할 수 있도록 하는 관리 계층**이다.

`bms-package`가 패키지 자체의 포맷과 내용을 다룬다면, 이 프로젝트는 패키지가 **어디에 있고, 어떤 state가 설치되어 있으며, 어떻게 관리되는가**를 담당한다.

```text
bms-package
    └── "이 파일은 어떤 패키지인가?"

bms-package-manager
    └── "이 패키지를 어디에 설치하고 어떻게 관리할 것인가?"
```

---

## 2. 책임 범위

### 담당

* 패키지 설치
* 패키지 제거
* 설치된 패키지 조회
* 패키지 state 관리
* 설치 상태 관리
* 패키지 저장 위치 관리
* 패키지 무결성 확인
* 패키지 업데이트
* 로컬 패키지 파일에서 설치
* Package Registry 관리
* 향후 Repository에서 패키지 다운로드
* 설치된 패키지의 discovery

### 비담당

* `.bmsp` 포맷 정의
* ZIP 처리
* manifest parsing
* BMS parsing
* BMS chart validation
* 음악 재생
* 이미지/오디오 decoding
* Beetle UI
* BMS 콘텐츠 자체의 의미 해석

---

## 3. 핵심 설계 원칙

### 3.1 Package Format과 Manager의 분리

Manager는 반드시 `bms-package`를 통해 패키지를 읽는다.

```text
.bmsp
  ↓
bms-package
  ↓
Package
  ↓
bms-package-manager
```

Manager가 ZIP 파일을 직접 다루거나 manifest JSON을 직접 parsing해서는 안 된다.

---

### 3.2 Installed Package는 immutable

설치가 완료된 패키지의 콘텐츠는 기본적으로 수정하지 않는다.

```text
Package
    ↓ install
Installed Package
    ↓
read-only content
```

패키지 내부 파일을 변경해야 한다면 기존 패키지를 제거하고 새로운 state를 설치한다.

---

### 3.3 Manager State와 Package Content의 분리

Manager가 관리하는 정보:

```text
id
state_hash
install location
installation time
source
```

Package가 가지고 있는 정보:

```text
manifest
BMS files
audio
images
other resources
```

Manager database에 BMS metadata를 복제하지 않는다.

---

## 4. Installation Model

패키지는 Package Manager의 managed directory에 설치한다.

```text
packages/
    example.song/
        1.0.0/
            manifest.json
            bms/
            audio/
            image/
```

```text
Package ID
    └── State
          └── Immutable package content
```

한 패키지의 여러 state가 동시에 존재할 수 있어야 한다.

---

## 5. Package Identity

Package identity는 다음 조합으로 결정한다.

```text
PackageId + State
```

예:

```text
example.song:a3f8c2
example.song:7b1d0e
```

`bms-package`에서 제공하는 `id`와 `state_hash`를 authoritative value로 사용한다.

---

## 6. Registry

Package Manager는 설치 상태를 추적하기 위한 local registry를 가진다.

개념적으로:

```text
Registry
├── example.song
│   ├── a3f8c2
│   └── 7b1d0e
└── another.song
    └── e9c4a1
```

Registry는 다음 질문에 답할 수 있어야 한다.

* 어떤 package가 설치되어 있는가?
* 특정 package의 어떤 state가 설치되어 있는가?
* 특정 state의 설치 위치는 어디인가?
* 현재 active state는 무엇인가?

---

## 7. Registry Storage

권장 형태: `registry.json`

```json
{
  "packages": {
    "example.song": {
      "active_state": "7b1d0e",
      "states": {
        "a3f8c2": {
          "path": "packages/example.song/1.0.0",
          "installed_at": "2026-08-28T02:00:00Z"
        },
        "7b1d0e": {
          "path": "packages/example.song/1.1.0",
          "installed_at": "2026-08-28T02:30:00Z"
        }
      }
    }
  }
}
```

---

## 8. Install

로컬 `.bmsp` 파일을 설치한다.

설치 과정:

```text
.bmsp
  ↓
Open with bms-package
  ↓
Validate
  ↓
Read manifest
  ↓
Determine package ID/state
  ↓
Check existing installation
  ↓
Extract/copy to managed storage
  ↓
Verify installation
  ↓
Update registry
```

---

## 9. Atomic Installation

```text
temporary directory
        ↓
extract/install
        ↓
verify
        ↓
atomic move
        ↓
registry update
```

---

## 10. Atomic Delta Update 파이프라인 (`updater.rs`)

`bms-package-manager`는 차분 패키지(`.bmdp`)를 기존에 설치된 Base 패키지에 원자적으로 적용하여 타겟 버전을 생성·설치한다.

```text
1단계: Base State 검증 (Installed State vs Delta.base_state_hash)
        ↓
2단계: 격리된 Staging 디렉터리(.staging/)에서 Delta 적용 및 타겟 재현
        ↓
3단계: 복원된 Target Package SHA-256 및 Target Manifest 무결성 검증
        ↓
4단계: 원자적 설치(Atomic Commit) 및 registry.json 버전/해시 갱신
```

* **원자적 안전성 (Atomic Safety)**:
  * Delta 적용 도중 예외, 체크섬 불일치, 프로세스 강제 종료가 발생해도 기존에 설치된 패키지는 100% 무손상으로 보존됩니다.
  * 복원 실패 시 `BaseStateNotInstalled` 또는 `MismatchedBaseState` 오류를 반환하며, Full Package 재설치(Fallback)로 투명하게 전환 가능합니다.

---

## 11. Duplicate Installation

이미 동일한 package ID와 state가 설치되어 있는 경우 `AlreadyInstalled` 오류를 반환한다.

---

## 12. Uninstall

```text
manager.uninstall("example.song", "1.0.0")
```

---

## 13. Multiple States & Active State

동일 package의 여러 버전(State)을 보관하며, package ID마다 하나의 active state를 지정할 수 있다.

---

## 14. Discovery API

Beetle 및 외부 런타임이 사용할 수 있는 패키지 조회 API를 제공한다.

```rust
manager.list_packages() -> Vec<PackageSummary>
manager.get_package(id) -> Option<PackageRecord>
manager.get_installed_states(id) -> Vec<String>
manager.get_active_package(id) -> Option<InstalledPackage>
```

```rust
pub struct InstalledPackage {
    pub id: String,
    pub state_hash: String,
    pub location: PathBuf,
}
```

---

## 15. CLI (`bpm`) 명세

```bash
# 1. BMS 폴더를 .bmsp 파일로 패킹
bpm pack ./songs/my_song/ -o my_song-1.0.0.bmsp

# 2. 기존 BMS 폴더를 패키지 관리자로 즉시 임포트 & 설치
bpm import ./songs/my_song/

# 3. 로컬 .bmsp 패키지 파일 설치
bpm install ./my_song-1.0.0.bmsp

# 4. 차분(Delta, .bmdp) 생성
bpm diff base-1.0.0.bmsp target-1.1.0.bmsp -o patch-1.1.0.bmdp
bpm pack ./songs/my_song_v2/ --base base-1.0.0.bmsp -o patch-1.1.0.bmdp

# 5. 차분 패치 적용 및 원자적 업데이트
bpm patch base-1.0.0.bmsp patch-1.1.0.bmdp -o target-1.1.0.bmsp
bpm update patch-1.1.0.bmdp

# 6. 설치된 패키지 목록 조회
bpm list

# 7. 패키지 상세 정보 및 버전 목록 확인
bpm info <package_id>

# 8. 활성 버전 전환
bpm activate <package_id> <version>

# 9. 패키지 버전 삭제
bpm uninstall <package_id> <version>
```

---

## 16. 독립형 GUI 매니저 (`bpm-gui`)

* **개요**: 터미널 없이 데스크톱에서 독립 실행되는 경량 패키지 관리자 GUI.
* **기능**:
  * 패키지 목록 및 버전 탐색, 실시간 CJK 검색 필터 (`[/]`)
  * `.bmsp` / `.bmdp` 파일 드래그 앤 드롭 또는 1-클릭 설치 (`[D]`/`F3`)
  * 차분 제작 마법사 모달 (`[C]`/`F4`): Base 곡과 타겟 폴더 선택 시 1-클릭 `.bmdp` 생성
  * 백그라운드 Worker 스레드 + 회전 스피너를 통한 60 FPS 논블로킹 UI (`INV-5`)

---

## 17. Security & Non-goals

* Path traversal 방어 (`validate_entry_path`)
* Symlink/Absolute path 차단
* Safe atomic extraction
* Remote repo / DRM / Account는 비담당 (향후 확장 제안서 참조)
