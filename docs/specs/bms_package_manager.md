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

## 10. Duplicate Installation

이미 동일한 package ID와 state가 설치되어 있는 경우 `AlreadyInstalled` 오류를 반환한다.

---

## 11. Uninstall

```text
manager.uninstall("example.song", "a3f8c2")
```

---

## 12. Multiple States & 13. Active State

동일 package의 여러 state를 보관하며, package ID마다 하나의 active state를 지정할 수 있다.

---

## 14. Discovery API

Beetle이 사용할 수 있는 최소 API를 제공한다.

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

## 15. Beetle Integration

```text
Beetle
  ↓
Package Manager
  ↓
InstalledPackage
  ↓
bms-package
  ↓
BMS file
```

---

## 16. CLI (`bpm`)

```text
bpm install <path.bmsp>
bpm list
bpm info <package_id>
bpm states <package_id>
bpm uninstall <package_id> [state]
bpm activate <package_id> <state>
```

---

## 17. Security & Non-goals

* Path traversal 방어 (`validate_entry_path`)
* Symlink/Absolute path 차단
* Safe atomic extraction
* Remote repo / DRM / Account는 비담당
