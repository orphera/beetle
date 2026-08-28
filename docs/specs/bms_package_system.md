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

---

# 2. 핵심 개념

시스템에서 다음 개념을 명확하게 분리한다.

```text
Package
Package Version
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

그리고 차분은 Package 자체의 정체성이 아니라 **Package Version 사이의 변화를 표현하는 전달 단위**다.

```text
Package
  │
  ├── Version 1
  ├── Version 2
  └── Version 3

Version 1 ──delta──> Version 2
Version 2 ──delta──> Version 3
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

Package는 안정적인 식별자를 가진다.

```yaml
id: example.artist.song
version: 1.2.0
```

`id`와 `version`은 서로 다른 개념이다.

```text
example.artist.song@1.0.0
example.artist.song@1.1.0
example.artist.song@1.2.0
```

위 세 개는 서로 다른 Package가 아니라 **동일한 Package의 서로 다른 Version**이다.

Package Manager는 파일명이나 설치 경로를 identity로 사용하지 않는다.

---

# 5. Package Version

Version은 특정 시점의 Package 전체 상태를 식별한다.

중요한 것은 Package Version을 **현재 설치된 파일들의 우연한 상태**가 아니라 재현 가능한 하나의 상태로 취급하는 것이다.

따라서 Version은 다음과 같은 정보를 결정할 수 있어야 한다.

```text
Package ID
Package Version
Manifest
Charts
Resource Set
Dependency Set
```

Package Manager는 이 Version을 기준으로 update 여부를 판단한다.

---

# 6. Package Manifest

Manifest는 Package의 구조와 의미를 정의한다.

예:

```yaml
package:
  id: example.artist.song
  version: 1.2.0

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

# 10. Delta는 Version 간 변환이다

Delta는 독립적인 Package가 아니다.

```text
Base Version
     │
     │ Delta
     ▼
Target Version
```

예:

```text
example.song@1.0.0
        │
        │ delta
        ▼
example.song@1.1.0
```

따라서 Delta에는 최소한 다음 개념이 필요하다.

```text
package_id
base_version
target_version
delta_format
delta contents
integrity information
```

---

# 11. Delta의 전제 조건

Delta는 아무 Package에나 적용할 수 있는 것이 아니다.

```text
Base Version
     │
     │ exact match
     ▼
Delta
     │
     ▼
Target Version
```

Manager는 현재 설치된 Version이 Delta가 요구하는 `base_version`과 정확히 일치하는지 확인한다.

잘못된 Version에 Delta를 적용하려고 하지 않는다.

예:

```text
Installed: 1.0.0
Delta:     1.1.0 → 1.2.0
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

즉 Delta 적용 결과는 완전한 Target Version이어야 한다.

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

여러 Version이 존재할 수 있다.

```text
1.0 ──→ 1.1 ──→ 1.2 ──→ 1.3
```

따라서 Manager는 필요한 경우 Delta Chain을 사용할 수 있다.

```text
1.0
 ↓
delta 1.0→1.1
 ↓
1.1
 ↓
delta 1.1→1.2
 ↓
1.2
```

다만 Delta Chain이 무한히 길어지는 것은 바람직하지 않다.

Repository는 특정 Version에 대해 full Package를 제공할 수 있어야 하고, Manager는 다음을 비교해 더 합리적인 경로를 선택할 수 있다.

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

Repository는 가능하면 특정 Version의 완전한 Package를 제공할 수 있어야 한다.

```text
                ┌── Full Package ──┐
Repository ─────┤                  ├──> Installed
                └── Delta ─────────┘
```

이것이 중요한 이유는 다음과 같다.

* 최초 설치
* 오래된 Version에서 업데이트
* Delta가 존재하지 않는 Version
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
  ├── version
  ├── size
  └── checksum

Delta
  ├── base_version
  ├── target_version
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

최종 결과가 기대한 Target Version과 일치하지 않는 경우 설치를 성공으로 처리하지 않는다.

---

# 18. Atomic Update

Delta를 적용하는 과정에서 프로그램이 종료되거나 오류가 발생해도 기존 Package를 망가뜨려서는 안 된다.

권장 흐름:

```text
Installed v1
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
Installed v2
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
example.song@1.2.0
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

Repository는 Package 및 Version을 제공하는 공급원이다.

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
version discovery
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
Version
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

Version graph를 다음처럼 생각할 수 있다.

```text
       delta
1.0 ─────────→ 1.1
 │             │
 │             │ delta
 │             ▼
 └──────────→ 1.2
      full
```

Manager는 현재 Version에서 목표 Version까지 갈 수 있는 경로를 선택한다.

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

Repair는 가능하면 해당 Version의 Full Package를 기준으로 수행한다.

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

동일한 Package Version은 가능한 한 동일한 결과를 생성해야 한다.

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

# 33. Format Version과 Package Version

둘은 반드시 구분한다.

```text
Package Format Version
    = Package 구조 자체의 버전

Package Version
    = 작품의 버전
```

예:

```text
format = 2
package = 1.4.0
```

Format이 변경되더라도 Package Version과 동일한 의미를 갖지 않는다.

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
    ├── Version 1
    │
    ├── Version 2
    │      ▲
    │      │ Delta
    │      │
    └── Version 3
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
                    has versions
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

### 4. Delta는 Version 간 변환이다.

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

---

# 37. 구현 우선순위

처음부터 모든 기능을 구현하지 않는다.

### Phase 1 — Package

```text
Manifest
Package ID
Version
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
Version resolution
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

# 38. 최종 철학

이 시스템의 중심은 Package Manager가 아니다.

중심은 **BMS 작품을 안정적인 Package와 Version으로 정의하는 것**이다.

그 위에 다음 기능들이 자연스럽게 올라간다.

```text
                    BMS WORK
                       │
                       ▼
                    Package
                       │
              ┌────────┴────────┐
              ▼                 ▼
           Version             Metadata
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

> **Delta는 Package의 또 다른 형태가 아니라, 동일한 Package의 한 Version을 다른 Version으로 변환하는 방법이다.**

따라서 Package Manager가 Delta를 선택적으로 사용하더라도 Package와 Player의 모델은 복잡해지지 않아야 한다.

최종적으로 시스템이 지향하는 구조는 다음과 같다.

```text
                 ┌────────────────────┐
                 │    BMS Package     │
                 │                    │
                 │  Identity          │
                 │  Version           │
                 │  Manifest          │
                 │  Charts            │
                 │  Resources         │
                 └─────────┬──────────┘
                           │
                    Version changes
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
