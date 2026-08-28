# BMS Package System

## 1. 목적

BMS 작품을 하나의 독립적인 **Package**로 정의하고, 이를 저장·배포·설치·업데이트할 수 있는 공통 시스템을 제공한다.

전체 시스템은 다음 세 계층으로 나뉜다.

```text
BMS Package
    │
    ├── package format / metadata / resources
    │
    ├── delta
    │
    └── repository distribution
             │
             ▼
      BMS Package Manager
             │
             ▼
       Installed Packages
             │
             ▼
          BMS Player
```

핵심 목표는 다음과 같다.

* BMS 작품을 독립적인 단위로 표현한다.
* 작품의 버전과 변경분을 명확하게 표현한다.
* 전체 Package를 매번 재배포하지 않고 **차분(delta)** 으로 배포할 수 있다.
* Package의 배포와 설치를 Player로부터 분리한다.
* 이미 설치된 Package를 안전하게 업데이트할 수 있다.
* Package를 다른 BMS 프로그램에서도 사용할 수 있도록 한다.

> 📚 **세부 구현 명세서 및 책임 분리 (Detailed Specifications)**:
> - **[bms_package.md](bms_package.md)**: `.bmsp` 풀 패키지 & `.bmdp` 차분(Delta) 포맷, `DeltaManifest`, `DeltaBuilder`/`DeltaApplicator` 라이브러리 스펙
> - **[bms_package_manager.md](bms_package_manager.md)**: 원자적 차분 업데이트 파이프라인(`updater.rs`), `registry.json`, `bpm` CLI 및 `bpm-gui` 매니저 스펙

---

# 2. 핵심 개념

시스템에서 다음 개념을 명확하게 분리한다.

```text
Package
Package State
Package Delta
Archive
Repository
Installation
Package Manager
Player
```

특히 다음 관계를 유지한다.

```text
Package
  ≠ Archive
  ≠ Installation
  ≠ Repository
  ≠ Player
```

그리고 차분은 Package 자체의 정체성이 아니라 **Package State 사이의 변화를 표현하는 전달 단위**다.

```text
Package
  │
  ├── State a3f8c2...
  ├── State 7b1d0e...
  └── State e9c4a1...

a3f8c2 ──delta──> 7b1d0e
7b1d0e ──delta──> e9c4a1
```

---

# 3. Package

Package는 하나의 BMS 작품을 표현하는 논리적 단위다.

```text
Package
├── Manifest
├── Charts
├── Resources
│   ├── Audio
│   ├── Image
│   ├── Video
│   └── Other
└── Package Metadata
```

Package는 특정 Player의 내부 데이터 구조가 아니다.

따라서 다음과 같은 정보를 Package의 본질적인 데이터로 취급하지 않는다.

* 설치 경로
* Player 내부 database ID
* Player의 cache
* 사용자 설정
* 재생 기록
* UI 상태

---

# 4. Package Identity

Package는 두 수준의 식별자를 가진다.

```text
id          = 작품을 식별하는 논리적 이름 (사람이 부여)
state_hash  = Package 내용물의 SHA-256 해시 (내용에서 결정론적으로 도출)
```

예:

```text
example.artist.song : a3f8c2d1...
example.artist.song : 7b1d0e9f...
```

위 두 개는 서로 다른 Package가 아니라 **동일한 Package의 서로 다른 State**다.

`id`는 사람이 부여하는 논리적 그룹이다. `state_hash`는 내용물에서 자동으로 결정된다.

```text
State Hash = SHA-256(canonical archive bytes)
```

동일한 내용물은 항상 동일한 해시를 생성한다(INV-6 결정론적 패키징).

Package Manager는 파일명이나 설치 경로를 identity로 사용하지 않는다.

---

# 5. Package State

State는 특정 시점의 Package 전체 상태를 식별한다.

State 번호를 사람이 부여하는 대신, **Package 내용물의 해시가 곧 State의 식별자**다.

```text
State Hash = SHA-256(canonical archive bytes)
```

이는 git의 commit hash와 동일한 원리다.

```text
git commit = SHA-1(tree + parent + author + message)
Package State = SHA-256(manifest + charts + resources)
```

이 모델에서는 다음과 같은 문제가 구조적으로 사라진다.

```text
"누가 v1.1.0을 올릴 권한이 있는가?"  → 질문 자체가 없음
"같은 v1.1.0이 두 개 존재하면?"      → 내용이 다르면 해시가 다름
"v1.0.0과 v1.1.0 중 어느 것이 진짜?" → 해시가 곧 정체성
```

State에는 다음 정보가 포함된다.

```text
Package ID
State Hash (computed, not stored in manifest)
Manifest
Charts
Resource Set
```

선택적으로 다음을 Manifest에 포함할 수 있다.

```text
label       사람이 읽을 수 있는 이름 ("initial release", "ANOTHER 추가")
parent      이전 State의 해시 (변경 이력 추적, optional)
created_at  생성 시점의 타임스탬프 (optional)
```

`parent`를 통해 State 간의 시간 순서와 변경 이력을 추적할 수 있다.

```text
a3f8c2 (label: "initial release", parent: null)
   │
   ▼
7b1d0e (label: "ANOTHER 추가", parent: a3f8c2)
   │
   ▼
e9c4a1 (label: "키음 수정", parent: 7b1d0e)
```

---

# 6. Package Manifest

Manifest는 Package의 구조와 의미를 정의한다.

예:

```yaml
package:
  id: example.artist.song
  label: "ANOTHER 추가"
  parent: a3f8c2d1...
  created_at: 2024-02-01T00:00:00Z

metadata:
  title: Example Song
  artist: Example Artist

charts:
  - id: normal
    ...
  - id: hyper
    ...

resources:
  ...
```

Manifest에는 `version` 필드가 없다. State의 식별자는 아카이브 전체의 SHA-256 해시이며, Manifest 내부에 저장되지 않는다(순환 의존 방지).

Manifest는 Package의 canonical description이다.

filesystem 구조를 읽어서 Package의 의미를 추론하는 것을 기본 방식으로 삼지 않는다.

---

# 7. Resource

Package 내부의 파일은 단순한 filesystem path가 아니라 논리적인 Resource로 취급한다.

```text
Chart
  │
  ├── Audio Resource
  ├── BGA Resource
  └── Image Resource
```

Resource reference와 physical path를 분리한다.

```text
Logical Resource
       ↓
Package Resource
       ↓
Physical File
```

이를 통해 Package의 위치가 변경되어도 내부 reference가 깨지지 않는다.

---

# 8. Self-contained Package

기본적으로 Package는 필요한 Resource를 스스로 포함하는 것을 원칙으로 한다.

```text
Package
├── manifest
├── charts
├── audio
├── images
└── videos
```

Package를 다른 위치로 복사해도 동작할 수 있어야 한다.

다만 Resource 시스템 자체는 향후 shared/external resource를 표현할 수 있도록 지나치게 filesystem에 결합하지 않는다.

---

# 9. Package Delta

## 9.1 목적

BMS Package는 audio, BGA 등의 대용량 Resource를 포함할 수 있다.

따라서 작은 변경을 위해 Package 전체를 다시 배포하는 것은 비효율적이다.

예를 들어:

```text
v1.0
  2 GB

v1.1
  2 GB
```

에서 실제 변경량이 10 MB라면,

```text
Full Package Update
2 GB download
```

대신

```text
Delta
10 MB download
```

가 가능해야 한다.

---

# 10. Delta는 State 간 변환이다

Delta는 독립적인 Package가 아니다.

```text
Base State
     │
     │ Delta
     ▼
Target State
```

예:

```text
example.song : a3f8c2...
        │
        │ delta
        ▼
example.song : 7b1d0e...
```

따라서 Delta에는 최소한 다음 개념이 필요하다.

```text
package_id
base_hash
target_hash
delta_format
delta contents
integrity information
```

---

# 11. Delta의 전제 조건

Delta는 아무 Package에나 적용할 수 있는 것이 아니다.

```text
Base State
     │
     │ exact hash match
     ▼
Delta
     │
     ▼
Target State
```

Manager는 현재 설치된 State의 해시가 Delta가 요구하는 `base_hash`와 정확히 일치하는지 확인한다.

잘못된 State에 Delta를 적용하려고 하지 않는다.

예:

```text
Installed: a3f8c2...
Delta:     7b1d0e... → e9c4a1...
```

이 경우 Delta를 적용할 수 없다.

---

# 12. Delta는 전체 Package 상태를 생성할 수 있어야 한다

Delta 적용 결과는 단순히 변경된 파일의 집합이 아니다.

목표는 다음과 같다.

```text
Apply(
    Package@base,
    Delta(base → target)
)
=
Package@target
```

즉 Delta 적용 결과는 완전한 Target State이어야 한다.

이 특성을 통해 Delta를 적용한 결과와 Target Package를 직접 설치한 결과가 동일한 Package 상태가 되도록 한다.

---

# 13. Delta와 Resource

Delta는 변경의 성격에 따라 Resource 단위 변경을 표현할 수 있어야 한다.

예:

```text
v1.0
├── chart A
├── audio A
├── bga A
└── image A

v1.1
├── chart A       unchanged
├── audio A       changed
├── bga A         unchanged
└── image A       changed
```

Delta:

```text
unchanged:
  chart A
  bga A

changed:
  audio A
  image A
```

이렇게 하면 변경되지 않은 대용량 Resource를 다시 전달할 필요가 없다.

---

# 14. Delta의 구현 방식은 Package Format과 분리한다

Delta를 구현하는 구체적인 알고리즘은 Package의 논리적 개념과 분리한다.

가능한 구현은 여러 가지다.

```text
File-level delta
Binary delta
Resource replacement
Resource patch
Chunk-based delta
```

그러나 Package API가 특정 알고리즘에 강하게 결합되어서는 안 된다.

핵심 계약은 다음이다.

```text
Base Package
     +
Delta
     ↓
Target Package
```

---

# 15. Delta Chain

여러 State가 존재할 수 있다.

```text
a3f8c2 ──→ 7b1d0e ──→ e9c4a1 ──→ f2b7d3
```

따라서 Manager는 필요한 경우 Delta Chain을 사용할 수 있다.

```text
a3f8c2
 ↓
delta a3f8c2→7b1d0e
 ↓
7b1d0e
 ↓
delta 7b1d0e→e9c4a1
 ↓
e9c4a1
```

다만 Delta Chain이 무한히 길어지는 것은 바람직하지 않다.

Repository는 특정 State에 대해 full Package를 제공할 수 있어야 하고, Manager는 다음을 비교해 더 합리적인 경로를 선택할 수 있다.

```text
Full Package
vs
Delta
vs
Delta Chain
```

초기 구현에서는 복잡한 최적화보다 **정확한 Delta 적용과 Full Package fallback**을 우선한다.

---

# 16. Full Package는 항상 의미가 있다

Delta만으로 Package를 배포하는 시스템으로 만들지 않는다.

Repository는 가능하면 특정 State의 완전한 Package를 제공할 수 있어야 한다.

```text
                ┌── Full Package ──┐
Repository ─────┤                  ├──> Installed
                └── Delta ─────────┘
```

이것이 중요한 이유는 다음과 같다.

* 최초 설치
* 오래된 State에서 업데이트
* Delta가 존재하지 않는 State
* Delta 적용 실패
* 손상된 설치 복구
* 새로운 Manager 구현

즉 Delta는 **optimization**이고 Full Package는 **기본적인 전달 수단**이다.

---

# 17. Integrity

Package와 Delta 모두 integrity 정보를 가져야 한다.

예:

```text
Package
  ├── state_hash
  ├── size
  └── checksum

Delta
  ├── base_hash
  ├── target_hash
  ├── size
  └── checksum
```

특히 Delta에서는 다음 두 상태를 검증한다.

```text
Downloaded Delta
      ↓
Delta integrity
      ↓
Apply
      ↓
Target Package integrity
```

최종 결과가 기대한 Target State의 해시와 일치하지 않는 경우 설치를 성공으로 처리하지 않는다.

---

# 18. Atomic Update

Delta를 적용하는 과정에서 프로그램이 종료되거나 오류가 발생해도 기존 Package를 망가뜨려서는 안 된다.

권장 흐름:

```text
Installed (a3f8c2)
    │
    ▼
Download Delta
    │
    ▼
Verify Delta
    │
    ▼
Create temporary target
    │
    ▼
Apply Delta
    │
    ▼
Validate target
    │
    ▼
Atomic Commit
    │
    ▼
Installed (7b1d0e)
```

즉 다음 상태는 허용하지 않는다.

```text
half-updated package
```

---

# 19. Installation

Package와 Installation을 분리한다.

Package:

```text
example.song : 7b1d0e...
```

Installation:

```text
installed
path = ...
source = ...
```

Installation은 Package Manager가 관리한다.

Package 자체에는 다음 정보를 넣지 않는다.

```text
install_path
installed_at
player_database_id
```

---

# 20. Package Manager

Package Manager는 Package의 lifecycle을 관리한다.

주요 책임:

```text
discover
resolve
download
verify
install
update
remove
repair
```

Package 자체의 의미를 정의하는 것은 Manager의 책임이 아니다.

---

# 21. Repository

Repository는 Package 및 State을 제공하는 공급원이다.

```text
Repository
├── Package A
│   ├── v1.0
│   ├── v1.1
│   └── v1.2
│
└── Package B
    └── v1.0
```

Repository는 Full Package와 Delta를 모두 제공할 수 있다.

```text
Repository
 ├── Full Package
 └── Delta
      ├── 1.0 → 1.1
      ├── 1.1 → 1.2
      └── ...
```

Package identity는 Repository identity와 독립적이다.

---

# 22. Repository와 Package Manager의 분리

Manager는 특정 Repository의 구현에 종속되지 않는다.

```text
              Package Manager
              /      |      \
             /       |       \
       Local Repo  HTTP Repo  Other Repo
```

Repository는 최소한 다음 정보를 제공할 수 있는 추상화로 본다.

```text
package discovery
state discovery
package retrieval
delta retrieval
```

---

# 23. Cache

Download Cache와 Installation을 구분한다.

```text
Repository
    ↓
Download Cache
    ↓
Verify
    ↓
Installation
```

Cache에 파일이 존재한다고 해서 Package가 설치된 것은 아니다.

또한 Cache는 동일한 Package/Delta를 반복 다운로드하지 않기 위한 최적화 계층이다.

---

# 24. Player

BMS Player는 Package를 소비한다.

```text
Package
   ↓
BMS Player
   ↓
Playback
```

Player는 Package Manager의 내부 구현을 알 필요가 없다.

가능하면 다음 구조를 유지한다.

```text
              Package
              /     \
             /       \
            ▼         ▼
        Manager      Player
```

Player가 Package를 읽기 위해 반드시 Manager를 실행해야 하는 구조로 만들지 않는다.

---

# 25. Offline

설치가 완료된 Package는 인터넷 연결 없이 사용할 수 있어야 한다.

```text
Internet
   │
   ▼
Package Manager
   │
   ▼
Installed Package
   │
   ▼
BMS Player
```

Player는 playback을 위해 Repository에 접근하지 않는다.

---

# 26. Package Library

`bms-package`는 가능한 한 순수한 library로 유지한다.

담당:

```text
Package Model
Manifest
Resource
State
Validation
Delta Representation
Delta Application
Package Read/Write
```

가능한 한 직접 담당하지 않는 것:

```text
Network
Repository
Database
UI
Player State
Background Service
```

이렇게 해야 Player, Manager, 분석 도구 등 여러 프로그램에서 재사용할 수 있다.

---

# 27. Package Manager 구조

권장 구조:

```text
bms-package-manager
│
├── Registry
│
├── Repository
│
├── Resolver
│
├── Downloader
│
├── Cache
│
├── Installer
│
├── Updater
│
└── CLI
```

각 계층은 명확한 책임을 가진다.

```text
CLI
 ↓
Manager API
 ↓
Resolver / Installer / Updater
 ↓
Repository / Cache
 ↓
bms-package
```

CLI에 핵심 로직을 넣지 않는다.

---

# 28. Update

일반적인 Update 흐름:

```text
Installed v1.0
      │
      ▼
Repository Query
      │
      ▼
Latest v1.2
      │
      ▼
Find update path
      │
      ├── Full Package
      │
      └── Delta
             │
             ▼
          Download
             │
             ▼
           Verify
             │
             ▼
          Apply
             │
             ▼
          Validate
             │
             ▼
        Atomic Commit
```

Manager는 Delta가 존재한다면 사용할 수 있지만, Delta 자체를 반드시 사용해야 하는 것은 아니다.

---

# 29. Update Path

State graph를 다음처럼 생각할 수 있다.

```text
       delta
1.0 ─────────→ 1.1
 │             │
 │             │ delta
 │             ▼
 └──────────→ 1.2
      full
```

Manager는 현재 State에서 목표 State까지 갈 수 있는 경로를 선택한다.

초기 구현에서는 다음 정도면 충분하다.

1. 정확히 일치하는 Delta가 있으면 사용
2. 그렇지 않으면 Full Package 사용
3. 여러 Delta를 이어야 한다면 제한된 범위에서 Chain 사용
4. 실패하면 기존 Installation을 유지

복잡한 최적화는 이후의 문제다.

---

# 30. Repair

설치된 Package가 손상되었을 경우 Manager는 이를 탐지할 수 있어야 한다.

```text
Installed Package
      │
      ▼
Verify
      │
 ┌────┴────┐
 │         │
OK       Broken
           │
           ▼
         Repair
```

Repair는 가능하면 해당 State의 Full Package를 기준으로 수행한다.

Delta는 복구 수단보다 **정상적인 Update 최적화 수단**으로 취급한다.

---

# 31. Security Boundary

Package는 기본적으로 **데이터**다.

Package 설치 과정에서 임의 코드를 실행하지 않는다.

따라서 기본 Package에는 다음과 같은 기능을 두지 않는다.

```text
install script
post-install script
arbitrary executable
```

Package Manager 역시 Package를 설치한다는 이유로 Package 내부 코드를 실행하지 않는다.

---

# 32. Determinism

동일한 Package State은 가능한 한 동일한 결과를 생성해야 한다.

Delta 역시:

```text
Base + Delta
```

가 항상 동일한 Target을 생성하도록 한다.

이는 다음 기능에 중요하다.

* checksum
* cache
* reproducibility
* integrity verification
* debugging

---

# 33. Format Version과 Package State

둘은 반드시 구분한다.

```text
Package Format Version
    = Package 구조 자체의 버전

Package State
    = 작품의 버전
```

예:

```text
format = 2
package = 1.4.0
```

Format이 변경되더라도 Package State과 동일한 의미를 갖지 않는다.

---

# 34. Compatibility

Package Format이 발전해도 기존 Package를 가능한 한 오래 사용할 수 있도록 한다.

필요한 경우:

```text
Format v1
   ↓
Reader / Migration
   ↓
Format v2
```

그러나 초기 구현에서 미래의 모든 migration을 추상화하지 않는다.

현재 필요한 format을 명확하게 정의하고, 이후 확장 가능한 경계를 유지한다.

---

# 35. 권장 데이터 관계

전체 시스템의 핵심 관계는 다음과 같다.

```text
Package ID
    │
    ├── State 1
    │
    ├── State 2
    │      ▲
    │      │ Delta
    │      │
    └── State 3
           ▲
           │ Delta
           │
       Repository
           │
           ▼
    Package Manager
           │
           ▼
      Installation
           │
           ▼
        BMS Player
```

더 정확하게 표현하면:

```text
                ┌──────────────────┐
                │      Package     │
                │                  │
                │ id               │
                │ metadata         │
                │ charts           │
                │ resources        │
                └────────┬─────────┘
                         │
                    has states
                         │
             ┌───────────┼───────────┐
             ▼           ▼           ▼
            v1.0        v1.1        v1.2
                         ▲           ▲
                         │           │
                    Delta 1.0→1.1    │
                                     │
                              Delta 1.1→1.2

                         │
                         ▼
                  Package Manager
                         │
                         ▼
                    Installation
                         │
                         ▼
                       Player
```

---

# 36. 설계의 핵심 불변조건

구현 과정에서 다음 조건을 우선적으로 지킨다.

### 1. Package는 독립적인 작품 단위다.

```text
Package ≠ Player
```

### 2. Installation은 Package와 다르다.

```text
Package ≠ Installation
```

### 3. Archive는 운반 형식일 뿐이다.

```text
Package ≠ ZIP
```

### 4. Delta는 State 간 변환이다.

```text
Base + Delta = Target
```

### 5. Delta 적용 실패는 기존 Installation을 파괴하지 않는다.

```text
v1 + failed delta
    ↓
still v1
```

### 6. Full Package는 항상 유효한 fallback이다.

```text
Delta = optimization
Full Package = fundamental distribution unit
```

### 7. Package Manager는 Player가 아니다.

```text
Manager → lifecycle
Player  → playback
```

### 8. Player는 Package Manager 없이 Package를 사용할 수 있어야 한다.

### 9. Package Library는 Repository/Network/UI에 종속되지 않는다.

### 10. Package 설치 과정에서 임의 코드를 실행하지 않는다.

### 11. 제3자 차분은 독립 Package로 표현하는 것을 권장한다.

```text
Third-party sabun → 독립 Package (별도 id) 권장
동일 id의 State 추가 → 해당 Package 관리 주체의 몫
포맷 수준의 저자 인증 → 제공하지 않음 (배포 계층의 책임)
```

### 12. Delta 적용 실패는 항상 Full Package Fallback으로 복구된다.

```text
Delta fail → Full Package → 정상 설치
```

---

# 37. 구현 우선순위

처음부터 모든 기능을 구현하지 않는다.

### Phase 1 — Package

```text
Manifest
Package ID
State
Charts
Resources
Validation
Read / Write
```

### Phase 2 — Distribution

```text
Archive
Checksum
Repository metadata
Full Package retrieval
```

### Phase 3 — Manager

```text
Registry
Install
Remove
List
Verify
```

### Phase 4 — Delta

```text
Delta representation
Base/Target validation
Delta generation
Delta application
Integrity verification
```

### Phase 5 — Update

```text
State resolution
Delta selection
Atomic update
Fallback to full package
Repair
```

### Phase 6 — Optimization

```text
Delta chains
Chunking
Cache optimization
Repository-side optimization
```

---

# 38. Third-Party Derivative (제3자 차분/Sabun)

BMS 생태계에서는 원작자가 아닌 제3자가 난이도표용 채보(차분, いわゆる差分)를 만들어 배포하는 관행이 존재한다.

이 시스템에서 제3자 차분은 **두 가지 모델** 중 하나로 표현할 수 있다.

## 38.1 모델 A: 독립 Package (권장)

제3자가 만든 차분은 원본과 다른 `id`를 가진 별개의 Package로 정의한다.

```text
original.artist.song@1.0.0    ← 원작자의 Package
sabun.author.song-another@1.0.0  ← 제3자의 독립 Package
```

이 경우 Manifest에 의존 관계를 **선언적으로** 명시한다.

```yaml
package:
  id: sabun.author.song-another
  base_package:
    id: original.artist.song
    min_state: 1.0.0
```

`base_package`는 다음 의미를 가진다.

```text
base_package
  ├── id        : 원본 Package의 id
  ├── min_state : 최소 호환 State (optional)
  └── purpose   : 리소스 공유 선언 (채보만 추가, 키음은 원본 사용)
```

이 필드는 **정보 제공 목적**이며, Package Manager가 원본 Package의 키음 리소스를 공유하는 최적화에 활용할 수 있다.

중요한 점은 다음과 같다.

```text
base_package ≠ 원본의 State 체인에 합류
base_package ≠ 원본 Package의 Delta
```

제3자 Package는 원본의 State history를 오염시키지 않는다.

## 38.2 모델 B: 동일 Package의 Delta

동일한 `id`의 State chain에 Delta를 추가하는 것은 **해당 Package를 관리하는 주체**의 몫이다.

```text
original.artist.song@1.0.0
      │
      ▼
original.artist.song@1.1.0
```

제3자가 기존 곡에 채보를 추가하고 싶다면, 동일 `id`에 State을 올리는 것이 아니라 **모델 A(독립 Package)**를 사용하는 것을 권장한다.

```text
original.artist.song@1.1.0        ← 동일 id의 새 State
sabun.author.song-another@1.0.0   ← 별도 id의 독립 Package
```

## 38.3 Package Manager의 역할

Package Manager는 `base_package` 관계를 이용해 다음을 수행할 수 있다.

```text
1. 의존성 안내
   "이 패키지는 'original.artist.song' v1.0.0 이상이 필요합니다"

2. 리소스 공유 (선택적 최적화)
   설치 시 원본의 키음/BGA를 심볼릭 링크 또는 참조로 공유

3. 일괄 표시
   UI에서 원본과 파생 채보를 그룹으로 묶어 표시
```

그러나 Package Manager가 `base_package` 관계를 강제하지는 않는다.

```text
base_package 미설치 → 설치 차단 ❌
base_package 미설치 → 경고 표시 ✅
```

제3자 Package는 원본이 없어도 독립적으로 설치 가능해야 한다. 다만 키음이 누락되면 플레이에 지장이 있을 수 있음을 사용자에게 안내한다.

## 38.4 Identity와 신뢰

Package 포맷은 저자의 신원(Identity)을 검증하지 않는다.

```text
Package Format
  ├── id, state, manifest, resources
  └── 저자 인증 메커니즘 → 없음
```

`id`는 단순한 문자열이며, 포맷 수준에서 "이 id를 누가 소유하는가"를 기술적으로 강제하는 수단은 제공하지 않는다.

신원 검증이 필요한 경우 이는 **배포 계층(Repository / Registry)**의 책임이다.

```text
Package Format    → 구조와 무결성만 정의
Repository        → 네임스페이스 관리, 업로드 권한, 신뢰 정책
커뮤니티          → 배포처 신뢰, 사회적 검증
```

초기 구현에서는 이 경계를 유지하고, 향후 Repository 구현 시 네임스페이스 소유권이나 선택적 서명 필드를 도입할 수 있다.

---

# 39. Delta Integrity Resilience (Delta 무결성 회복력)

## 39.1 문제: Base 불일치에 의한 Fallback 쏠림

Delta 적용은 Base Package의 SHA-256 체크섬이 정확히 일치해야 한다.

```text
Expected Base checksum: abc123...
Actual   Base checksum: def456...
→ Delta 적용 실패
→ Full Package Fallback
```

이론상 결정론적 패키징(INV-6)에 의해 동일 소스에서 동일 아카이브가 생성되어야 하지만, 현실에서는 다음 원인으로 Base 불일치가 발생할 수 있다.

```text
1. 사용자가 설치된 파일을 수동 편집
2. 디스크 오류에 의한 비트 부패
3. 이전 버전의 Builder가 다른 정렬/타임스탬프를 사용
4. 다른 Package Manager 구현이 다른 방식으로 설치
```

## 39.2 완화 전략

### 전략 1: Full Package Fallback은 항상 유효하다 (현재 구현)

```text
Delta 적용 시도
     │
     ├── 성공 → Target 설치
     │
     └── 실패 (Base 불일치)
              │
              ▼
         Full Package 다운로드
              │
              ▼
         Target 설치
```

이것이 가장 단순하고 안전한 전략이며, 초기 구현에서는 이것으로 충분하다.

### 전략 2: Content-Addressable Resource 매칭 (향후 최적화)

Base Package 전체의 체크섬 대신, **개별 리소스 단위**로 매칭하는 방식이다.

```text
Delta Manifest:
  resource "bgm.wav"
    expected_base_checksum: abc123...
    actual base resource:   abc123... ✅ 일치

  resource "chart.bms"
    expected_base_checksum: def456...
    actual base resource:   def456... ✅ 일치
```

이렇게 하면 Manifest JSON의 공백이나 정렬이 달라도 개별 리소스가 일치하면 Delta를 적용할 수 있다.

다만 이 전략은 복잡성이 증가하므로 초기 구현에서는 도입하지 않는다.

### 전략 3: Canonical Package Normalization (향후 최적화)

설치된 Package를 Canonical Form으로 재빌드한 후 체크섬을 비교하는 방식이다.

```text
Installed files
     │
     ▼
Canonical Builder
     │
     ▼
Normalized Package (deterministic)
     │
     ▼
Checksum 비교
```

INV-6(결정론적 패키징)이 이미 보장하는 정렬과 타임스탬프를 활용하면, 설치된 파일들로부터 원본과 동일한 체크섬을 복원할 수 있다.

## 39.3 Fallback 비용 최소화

Full Package Fallback이 빈번하게 발생하면 Delta 시스템의 의미가 퇴색된다.

이를 방지하기 위해 다음을 권장한다.

```text
1. Package 설치 시 원본 아카이브(.bmsp)를 캐시에 보관
   → 재빌드 없이 Base 체크섬 즉시 검증 가능

2. Delta 적용 전 Base 체크섬을 미리 검증
   → 실패가 예상되면 즉시 Full Package 경로로 전환
   → 불필요한 Delta 다운로드 방지

3. Repository는 최신 N개 State에 대해 Full Package를 유지
   → Delta Chain이 끊어져도 항상 복구 가능
```

## 39.4 설계 원칙

```text
Delta 적용 실패는 정상적인 시나리오다.
시스템은 이를 에러가 아닌 대체 경로(Fallback)로 처리한다.
Full Package는 항상 존재하며 항상 작동한다.
```

---

# 40. 최종 철학

이 시스템의 중심은 Package Manager가 아니다.

중심은 **BMS 작품을 안정적인 Package와 State으로 정의하는 것**이다.

그 위에 다음 기능들이 자연스럽게 올라간다.

```text
                    BMS WORK
                       │
                       ▼
                    Package
                       │
              ┌────────┴────────┐
              ▼                 ▼
           State             Metadata
              │
              ▼
            Delta
              │
              ▼
          Distribution
              │
              ▼
        Package Manager
              │
              ▼
         Installation
              │
              ▼
          BMS Player
```

그리고 차분 시스템의 핵심은 다음 한 문장으로 요약한다.

> **Delta는 Package의 또 다른 형태가 아니라, 동일한 Package의 한 State을 다른 State으로 변환하는 방법이다.**

따라서 Package Manager가 Delta를 선택적으로 사용하더라도 Package와 Player의 모델은 복잡해지지 않아야 한다.

최종적으로 시스템이 지향하는 구조는 다음과 같다.

```text
                 ┌────────────────────┐
                 │    BMS Package     │
                 │                    │
                 │  Identity          │
                 │  State           │
                 │  Manifest          │
                 │  Charts            │
                 │  Resources         │
                 └─────────┬──────────┘
                           │
                    State changes
                           │
                           ▼
                    ┌─────────────┐
                    │    Delta    │
                    │             │
                    │ base →      │
                    │ target      │
                    └──────┬──────┘
                           │
                           ▼
                    ┌─────────────┐
                    │ Repository  │
                    └──────┬──────┘
                           │
                           ▼
                 ┌──────────────────┐
                 │ Package Manager  │
                 │                  │
                 │ install          │
                 │ update           │
                 │ remove           │
                 │ verify           │
                 └────────┬─────────┘
                          │
                          ▼
                   ┌─────────────┐
                   │ Installation │
                   └──────┬──────┘
                          │
                          ▼
                    ┌───────────┐
                    │ BMS Player│
                    └───────────┘
```
