# BMS Package Specification (`bms-package`)

## 1. 목적

`bms-package`는 BMS 콘텐츠를 하나의 배포·보관 단위로 표현하기 위한 **패키지 포맷 및 라이브러리**다.

이 프로젝트는 다음을 담당한다.

* 패키지 포맷 정의
* 패키지 manifest 정의
* 패키지 생성 및 읽기
* 패키지 내부 파일 접근
* 패키지 무결성 검증
* 패키지의 기본 메타데이터 처리

다음은 책임 범위에서 제외한다.

* 패키지 설치/삭제
* 패키지 저장소(repository) 관리
* 버전 업데이트
* 의존성 해결
* 설치 위치 관리
* 사용자 계정/인증
* Beetle UI
* BMS 자체의 파싱 및 게임 로직

즉, `bms-package`는 **"패키지가 무엇이고 그 안에 무엇이 들어 있는가"**만 책임진다.

---

## 2. 설계 원칙

### 2.1 Package Format과 Package Manager의 분리

패키지 파일은 Package Manager 없이도 읽을 수 있어야 한다.

```text
Package File
    ↓
bms-package
    ↓
Package / Manifest / Entries
```

Package Manager는 이를 이용해 설치 및 관리 기능을 구현한다.

```text
Repository
    ↓
Package Manager
    ↓
bms-package
    ↓
Installed Content
```

`bms-package`가 Package Manager의 개념을 알아서는 안 된다.

---

### 2.2 콘텐츠와 설치 상태의 분리

패키지 내부에는 "설치되어 있는가", "어디에 설치되었는가"와 같은 상태를 저장하지 않는다.

패키지는 자기 자신만으로 의미가 결정되어야 한다.

---

### 2.3 단순성과 결정론적 설계
 
BMS 콘텐츠를 안정적으로 묶고 이동할 수 있는 최소한의 포맷을 정의한다.
BMS 작품의 대용량 재배포를 방지하기 위해 **결정론적 차분(Delta, `.bmdp`) 포맷 및 Diff/Patch 엔진**을 내장하여 지원한다.

---

## 3. Package 구조

패키지는 하나의 파일로 배포한다.

권장 확장자는:

```text
.bmsp
```

예:

```text
song-name-a3f8c2.bmsp
```

패키지는 ZIP 기반 container를 사용한다.

```text
package.bmsp
├── manifest.json
├── bms/
│   └── *.bms
├── audio/
│   └── ...
├── image/
│   └── ...
└── ...
```

ZIP을 사용하는 이유는 다음과 같다.

* 구현이 단순하다.
* 기존 도구로 내용을 검사할 수 있다.
* 압축과 비압축 파일을 모두 지원한다.
* 다양한 언어에서 구현하기 쉽다.

단, `.bmsp`가 ZIP이라는 사실을 애플리케이션의 API에 노출하지 않는다.

---

## 4. Manifest

패키지의 루트에는 반드시 다음 파일이 존재해야 한다.

```text
manifest.json
```

Manifest는 패키지 자체의 identity와 콘텐츠 정보를 기술한다. `state_hash`는 아카이브 전체의 SHA-256 해시로, 패키지 생성 시 자동으로 계산되어 저장된다(순환 의존 방지를 위해 manifest 내부에는 저장하지 않고, 패키지 읽기 시 계산하여 검증한다).

최소 형태:

```json
{
  "format": 1,
  "id": "example.song",
  "state_hash": "a3f8c2d1...",
  "name": "Example Song",
  "author": "Example Author"
}
```

---

### 4.1 필드

#### `format`

패키지 포맷 버전.

```json
"format": 1
```

정수다.

패키지 포맷 자체가 변경될 때 증가한다.

Package Manager가 사용하는 package state와 혼동해서는 안 된다.

---

#### `id`

패키지의 안정적인 식별자.

```json
"id": "example.song"
```

다음 조건을 만족해야 한다.

* 비어 있지 않아야 한다.
* Unicode normalization이나 대소문자 변환에 의존하지 않는다.
* 동일한 콘텐츠의 다른 버전은 동일한 `id`를 사용한다.
* 패키지의 `state_hash`와 결합하여 특정 상태를 식별할 수 있다.

예:

```text
example.song
example.artist.collection
```

첫 버전에서는 ID의 복잡한 namespace 규칙을 강제하지 않는다.

---


#### `state_hash`

패키지 상태를 식별하는 SHA-256 해시.

```json
"state_hash": "a3f8c2d1..."
```

아카이브 전체의 canonical bytes에 대한 SHA-256 해시이다. Manifest 내부에는 저장되지 않으며(순환 의존 방지), 패키지 읽기 시 계산되어 검증에 사용된다.

---

#### `name`

사용자에게 표시할 패키지 이름.

```json
"name": "Example Song"
```

표시용 문자열이며 식별자로 사용해서는 안 된다.

---

#### `author`

제작자 정보.

```json
"author": "Example Author"
```

선택 사항으로 취급할 수 있다.

---

## 5. Content

패키지 내부의 실제 콘텐츠는 일반 파일로 저장한다.

예:

```text
bms/
    example.bms

audio/
    bgm.ogg
    01.wav

image/
    stage.png
    banner.png
```

`bms-package`는 파일의 의미를 가능한 한 해석하지 않는다.

즉:

```text
audio/foo.wav
```

가 실제로 BMS에서 사용되는 오디오 파일인지 판단하는 것은 `bms-package`의 책임이 아니다.

BMS parser / Beetle이 처리한다.

---

## 6. Entry

패키지 내부 파일은 `Entry`로 표현한다.

개념적으로:

```text
Entry
 ├── Path
 ├── Size
 └── Content
```

API 사용자는 ZIP entry의 존재를 직접 알 필요가 없어야 한다.

예시 API:

```text
package.entries()
package.open_entry("bms/example.bms")
package.contains("audio/01.wav")
```

---

## 7. Path 규칙

패키지 내부 경로는 `/`를 사용한다.

허용:

```text
bms/example.bms
audio/01.wav
image/stage.png
```

허용하지 않는다:

```text
../foo
/absolute/path
C:\foo
```

경로 traversal은 반드시 거부한다.

또한 같은 파일을 서로 다른 경로 표현으로 접근할 수 있게 만드는 ambiguous normalization을 피한다.

예:

```text
a/../b
./b
```

등은 canonical path로 변환하기보다는 패키지 entry 자체에서 거부하는 것을 기본 정책으로 한다.

---

## 8. Encoding

Manifest는 반드시 UTF-8 JSON이어야 한다.

BMS 콘텐츠 자체의 encoding은 `bms-package`가 변환하지 않는다.

즉 패키징 과정에서:

```text
CP932 → UTF-8
```

같은 변환을 수행하지 않는다.

BMS parser가 콘텐츠의 encoding을 처리한다.

---

## 9. Integrity

패키지 파일은 읽는 과정에서 기본적인 무결성을 검증해야 한다.

최소한 다음을 검증한다.

* ZIP container가 정상적으로 열리는가
* `manifest.json`이 존재하는가
* manifest JSON이 유효한가
* 필수 필드가 존재하는가
* `format`이 지원되는 버전인가
* entry path가 유효한가
* 중복 entry가 존재하지 않는가

손상된 패키지는 정상적인 `Package` 객체로 노출하지 않는다.

---

## 10. Package API

라이브러리는 최소한 다음 작업을 제공한다.

### 읽기

```rust
Package::open(path)
Package::from_bytes(bytes)
Package::from_reader(reader)
```

패키지 파일을 열고 검증한다.

### Manifest 접근

```rust
package.manifest()
```

다음 정보를 얻을 수 있어야 한다.

```text
Format
Id
State Hash
Name
Author
```

### Entry 탐색

```rust
package.entries()
package.contains(path)
```

### 파일 열기 / 읽기

```rust
package.read_entry(path)
package.open_entry(path)
```

---

## 11. Package Creation API

패키지를 생성할 수 있어야 한다.

개념적으로:

```rust
PackageBuilder::new(manifest)
    .add_file(path, data)?
    .build_to_file(path)?;
```

Builder는 다음을 검증한다.

* manifest 유효성
* 중복 path
* 잘못된 path
* 빈 파일명
* 지원하지 않는 format version

---

## 12. Deterministic Build

동일한 입력으로 생성한 패키지가 가능하면 동일한 결과를 만들도록 한다.

이를 위해 다음 사항을 준수한다.

* Entry ordering을 알파벳순으로 deterministic하게 한다.
* Manifest serialization을 키 정렬 기반으로 deterministic하게 한다.
* ZIP metadata에 고정 epoch timestamp(`1980-01-01 00:00:00`)를 사용하여 불필요한 OS 시간이 들어가지 않도록 한다.
* 파일 순서를 입력 순서에 의존시키지 않는다.

---

## 13. BMS Package Delta 포맷 및 Diff/Patch 엔진 (`delta/`)

BMS 작품의 대용량 재배포를 방지하고 차분 제작자/원곡자의 배포 마찰을 제로화하기 위해 **결정론적 차분 아카이브(`.bmdp`)** 포맷과 코어 라이브러리를 제공한다.

### 13.1 Delta 아카이브 구조 (`.bmdp`)

```text
patch.bmdp
├── delta_manifest.json
└── resources/ (추가되거나 수정된 파일만 포함)
```

### 13.2 Delta Manifest 스키마 (`delta_manifest.json`)

```json
{
  "format": 1,
  "package_id": "example.song",
  "base_state_hash": "a3f8c2...",
  "target_state_hash": "7b1d0e...",
  "base_checksum": "sha256:...",
  "target_checksum": "sha256:...",
  "added_resources": ["bms/another.bme", "audio/keysound.wav"],
  "modified_resources": ["manifest.json"],
  "removed_resources": ["old_chart.bms"],
  "unchanged_resources": ["audio/bgm.ogg", "image/stage.bmp"]
}
```

### 13.3 결정론적 Diff/Patch 보장 (`INV-6`)

1. **`DeltaBuilder`**:
   * Base 패키지와 Target 패키지 간의 리소스/차트 diff를 추출하여 변경/추가된 리소스만 압축한 `.bmdp`를 생성합니다.
   * 사전순 정렬 및 고정 타임스탬프(`1980-01-01 00:00:00`)를 적용합니다.
2. **`DeltaApplicator`**:
   * `Base Package + Delta Archive`를 검증하고 `Target Package`를 100% 바이트 단위로 동일하게 복원합니다.
   * 불변식: $\text{Apply}(\text{Package@base}, \text{Delta}) = \text{Package@target}$

### 13.4 Delta 에러 정의 (`DeltaError`)

* `MismatchedBaseState { expected: String, actual: String }`
* `MismatchedTargetState { expected: String, actual: String }`
* `CorruptedPayloadChecksum`
* `InvalidDeltaManifest(String)`
* `MissingBaseResource(String)`
* `PackageError(PackageError)`

---

## 14. 오류 처리

라이브러리는 잘못된 패키지를 가능한 한 명확하게 구분한다 (`PackageError`).

* `Io(std::io::Error)`
* `InvalidZip(String)`
* `MissingManifest`
* `InvalidManifest(String)`
* `UnsupportedFormat(u32)`
* `InvalidEntryPath(String)`
* `DuplicateEntry(String)`
* `EntryNotFound(String)`
* `DecompressionLimitExceeded(u64)`
* `CorruptedPackage(String)`

---

## 15. Security

패키지는 외부에서 받은 untrusted input으로 취급한다.

반드시 방어해야 하는 항목:

* Path traversal (`..`)
* Absolute path (`/`, `C:\`)
* Duplicate entry
* 비정상적으로 큰 파일 (Zip bomb 방어 limit)
* Manifest parsing abuse

---

## 16. Dependency

첫 버전의 `bms-package`에서는 dependency를 정의하지 않는다.

---

## 17. Extension Strategy

Manifest는 향후 확장을 고려하여 알 수 없는 optional field를 보존/무시할 수 있다 (`serde(flatten)` extra).
반면 `format`처럼 package semantics를 결정하는 필드는 엄격하게 처리한다.

---

## 18. BMS-specific metadata

첫 버전에서는 BMS의 모든 메타데이터를 manifest에 복제하지 않는다. Authoritative source는 BMS 파일 자체다.

---

## 19. Non-goals

다음 기능은 이 프로젝트의 목표가 아니다.

* BMS parser
* BMS syntax validation
* BMS chart 분석
* Audio decoding
* Image decoding
* Song database
* Search engine
* Package repository
* Package installation
* Package update
* Dependency resolution
* User account
* Authentication
* DRM
* Content encryption

---

## 20. Beetle과의 Integration Boundary

```text
                 ┌───────────────┐
                 │ bms-package   │
                 └───────┬───────┘
                         │
              Package / Entry API
                         │
                         ▼
                 ┌───────────────┐
                 │ Package       │
                 │ Manager      │
                 └───────┬───────┘
                         │
                  Installed files
                         │
                         ▼
                 ┌───────────────┐
                 │    Beetle     │
                 └───────────────┘
```
