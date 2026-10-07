# BMS Library — Product Strategy

## 1. 문서 목적

이 문서는 기존 BMS 생태계에 새 BMS Package 시스템을 어떻게 침투시킬 것인지에 대한 제품 전략을 정리한다.

핵심 가설은 다음과 같다.

> 새로운 BMS 생태계를 처음부터 만들기보다, 사용자가 이미 가지고 있는 BMS 컬렉션을 관리하는 개인용 Library Manager로 진입한다.

Package와 Diff 시스템은 독립적인 목적이 아니라, 장기적으로 Library를 업데이트하고 동기화하는 기반으로 사용한다.

---

## 2. 문제 정의

현재 BMS 사용자는 다음과 같은 문제를 겪을 가능성이 있다.

### 2.1 파일 중심의 컬렉션 관리

BMS는 오랫동안 파일과 디렉터리를 중심으로 관리되어 왔다.

사용자는 다음을 직접 알아야 한다.

- 어느 폴더에 BMS가 있는가
- 어느 파일이 같은 곡인가
- 어떤 chart가 어느 패키지에서 왔는가
- 동일한 chart/resource가 여러 위치에 존재하는가
- 어떤 파일을 삭제해도 안전한가

즉, 파일 시스템은 실제 BMS 컬렉션의 의미 구조를 표현하지 못한다.

### 2.2 곡 / chart / resource 관계가 불명확하다

하나의 곡에는 여러 chart가 존재할 수 있고, chart는 여러 resource를 공유할 수 있다.

또한 동일한 chart가 서로 다른 패키지나 배포본에 포함될 수 있다.

따라서 단순한 파일 중복 검사는 충분하지 않다.

사용자가 실제로 알고 싶은 것은 다음에 가깝다.

> "이 파일이 무엇이며, 어디에서 왔고, 다른 파일과 어떤 관계가 있는가?"

### 2.3 컬렉션의 현재 상태를 알기 어렵다

사용자는 자신의 컬렉션이 다음 중 어떤 상태인지 쉽게 알기 어렵다.

- 중복 chart가 있는가
- 깨진 chart가 있는가
- resource가 누락되었는가
- 특정 난이도표를 얼마나 보유하고 있는가
- 오래된 배포본을 가지고 있는가
- 출처를 알 수 없는 파일이 얼마나 있는가

### 2.4 업데이트 비용

기존 방식에서는 패키지 전체를 다시 받거나 파일 단위로 교체하는 경우가 많다.

하지만 실제로 사용자가 원하는 것은:

> "내가 가진 것과 최신 버전 사이에서 무엇이 달라졌는가?"

이다.

따라서 hash 기반 identity와 diff가 실질적인 사용자 가치로 연결될 수 있다.

---

## 3. 기존 해결책과의 관계

### BMSeeker

BMSeeker는 강력한 기존 도구이며, 다음 영역에서 이미 상당한 기능을 제공한다.

- BMS 패키지 탐색 및 설치
- difficulty table 연동
- 곡 추가/삭제
- 중복 관련 관리
- 리소스 업데이트
- 패키지 관리
- 일부 DB/플레이어 연동

따라서 단순히 "BMSeeker보다 더 많은 기능"을 만드는 전략은 적절하지 않다.

### 우리가 노릴 영역

BMSeeker를 대체하기보다는 다른 추상화를 제공한다.

| 기존 접근 | BMS Library 접근 |
|---|---|
| BMS 파일/패키지 관리 | BMS 컬렉션 관리 |
| 무엇을 설치할 것인가 | 내가 무엇을 가지고 있는가 |
| 패키지 단위 | Song / Chart / Resource 관계 |
| 파일 조작 중심 | Library 모델 중심 |
| 다운로드 중심 | 상태 / provenance / synchronization 중심 |
| 특정 플레이어와 가까움 | Player-independent |
| 작업 실행 | 상태 파악 → Preview → 실행 |

핵심 차이는 **BMS 파일을 관리하는 것이 아니라 BMS 컬렉션을 모델링한다는 것**이다.

---

## 4. 제품 정의

### 제품명 가칭

**BMS Library**

Package Manager는 초기 제품명이 아니라 내부 기능 또는 후속 기능으로 취급한다.

### 한 문장 정의

> BMS Library는 사용자의 기존 BMS 컬렉션을 스캔하고, 곡·chart·resource의 관계와 상태를 파악하여 안전하게 관리하고 업데이트할 수 있게 하는 개인용 BMS 관리자다.

---

## 5. 핵심 설계 원칙

### 5.1 Existing Collection First

사용자가 이미 가진 파일을 존중한다.

처음부터 새로운 폴더 구조나 패키지 체계를 강요하지 않는다.

여러 위치에 흩어진 BMS도 하나의 Library로 볼 수 있어야 한다.

### 5.2 Read-only First

초기에는 기존 파일을 수정하지 않는다.

첫 경험은 다음과 같아야 한다.

```text
Scan
  ↓
Analyze
  ↓
Library View
  ↓
Preview
  ↓
Optional Action
```

사용자가 충분히 신뢰한 뒤에야 이동·삭제·설치 등의 작업을 수행한다.

### 5.3 Player Independent

LR2, beatoraja 등의 특정 플레이어에 종속되지 않는다.

플레이어는 Library를 소비하는 외부 시스템으로 취급한다.

### 5.4 Identity Before Location

파일 경로가 아니라 BMS 콘텐츠의 identity를 중심으로 관리한다.

예:

```text
Song
 ├─ Chart A
 │   ├─ Chart Identity
 │   └─ Resources
 ├─ Chart B
 └─ Chart C
```

동일한 콘텐츠가 다른 위치에 존재하더라도 동일한 identity로 인식할 수 있어야 한다.

### 5.5 Explain Before Modify

파일을 변경하기 전에 변경 이유와 영향 범위를 보여준다.

예:

```text
Remove: Package X / folder Y

384 charts
217 songs

Duplicate:
  1,204 charts

Shared resources:
  12 resources

Required by difficulty tables:
  4 charts

Estimated removable size:
  1.17 GB
```

---

# 6. MVP

MVP의 목적은 Package 생태계를 만드는 것이 아니다.

**"내 BMS 컬렉션을 한눈에 이해할 수 있다"**를 검증하는 것이다.

## 6.1 Library Scanner

지원:

- 여러 BMS root directory
- BMS
- BMSON
- chart/resource 관계 분석
- content hash
- 파일 metadata
- 기본적인 parse error 탐지

요구사항:

- 기존 파일 수정 금지
- 심볼릭 링크/중복 위치 처리 고려
- 대규모 Library에서도 incremental scan 가능
- 변경된 파일만 재분석

---

## 6.2 Library Database

최소 개념:

```text
Song
Chart
Resource
Location
Package
Source
DifficultyTable
```

관계:

```text
Song
 └─ Chart
     └─ Resource

Chart
 ├─ Location
 ├─ Package
 └─ DifficultyTable Entry
```

중요한 것은 파일 경로와 콘텐츠 identity를 분리하는 것이다.

---

## 6.3 Collection View

사용자가 다음 상태를 확인할 수 있어야 한다.

### 기본

- 전체 song 수
- 전체 chart 수
- resource 수
- 위치별 분포

### 문제 상태

- Duplicate
- Broken
- Missing Resource
- Unknown Origin
- Conflicting Version

### Difficulty Table

- Owned
- Missing
- Partial

---

## 6.4 Duplicate Analysis

단순한 동일 파일 검사를 넘어 다음을 구분한다.

```text
Exact Duplicate
Same Chart / Different Location
Same Chart / Different Resource Set
Potentially Same Song
Shared Resource
```

삭제는 MVP에서 선택적으로 제공하거나 Preview까지만 구현할 수 있다.

---

## 6.5 Difficulty Table Integration

difficulty table을 가져와 Library와 비교한다.

예:

```text
Satellite

Owned        1,204
Missing        183
Partial         21
Unknown          7

Coverage      87.4%
```

단순한 다운로드 목록이 아니라 **내 컬렉션 상태를 표현하는 기능**으로 사용한다.

---

# 7. Package 시스템과의 연결

Package는 MVP의 중심이 아니라 Library의 후속 기능이다.

## 기존 방식

```text
Package
   ↓
Download
   ↓
Extract
   ↓
Files
```

## 목표 방식

```text
Package
   ↓
Resolve identities
   ↓
Compare with Library
   ↓
Generate Change Set
   ↓
Preview
   ↓
Apply
```

예:

```text
Package: Example Pack 2026.10

+ 43 new charts
~ 127 resources updated
- 8 obsolete charts

Download:
1.8 GB → 94 MB

[Preview Changes]
```

이때 기존에 설계한 hash 기반 Package/Diff 시스템이 실제 사용자 가치로 연결된다.

---

# 8. Provenance

Library의 장기적인 차별점 중 하나.

각 콘텐츠가 어디에서 왔는지를 가능한 범위에서 추적한다.

예:

```text
Chart: Example Song [ANOTHER]

Origin:
  Example Package v1.2

Installed:
  2026-10-01

Current:
  v1.2

Latest known:
  v1.4

Update:
  18 MB
```

출처를 알 수 없는 기존 파일은:

```text
Origin: Unknown
```

으로 남긴다.

중요한 것은 provenance를 강제하지 않는 것이다.

**기존 컬렉션을 먼저 받아들이고, 앞으로 들어오는 콘텐츠부터 provenance를 축적한다.**

---

# 9. Library Health

장기적으로 Library의 핵심 화면이 될 수 있다.

예:

```text
BMS Library
────────────────────────

Charts              18,421
Songs                9,832

Duplicates            1,204
Broken                   37
Missing Resources       112
Outdated Packages       86
Unknown Origin        1,923
```

이 화면의 목적은 통계를 제공하는 것이 아니라:

> "내 BMS 컬렉션에 지금 무슨 일이 일어나고 있는가?"

를 알려주는 것이다.

---

# 10. Package Manager의 재정의

기존 계획에서는 `BMS Package Manager`가 독립 제품이었다.

새 전략에서는 다음 구조가 더 자연스럽다.

```text
                 BMS Library
                      │
       ┌──────────────┼──────────────┐
       ↓              ↓              ↓
 Difficulty        Package        Local Files
   Tables          Registry        / Folders
       │              │              │
       └──────────────┼──────────────┘
                      ↓
                Sync / Diff
                      ↓
               Change Preview
                      ↓
                   Apply
```

Package Manager는 Library가 제공하는 **Sync/Install 기능**으로 흡수할 수 있다.

---

# 11. 장기적인 생태계 침투 전략

## Stage 1 — Personal Utility

목표:

> "내 BMS 정리하는 데 유용하다."

기능:

- Scan
- Search
- Duplicate
- Health
- Difficulty Table

생태계 변화 없이 사용할 수 있어야 한다.

---

## Stage 2 — Collection Synchronization

목표:

> "내 컬렉션을 최신 상태로 유지하기 쉽다."

추가:

- Package identity
- Provenance
- Version
- Diff
- Update Preview
- Incremental Download

---

## Stage 3 — Package Ecosystem

목표:

> "BMS 제작자도 이 방식으로 배포하면 편하다."

추가:

- Package Registry
- Package publishing
- Versioning
- Differential update
- Dependencies
- Mirrors / federation

---

## Stage 4 — 기존 생태계와 공존

최종적으로는 다음 관계를 목표로 한다.

```text
                    BMS Library
                         │
          ┌──────────────┼──────────────┐
          ↓              ↓              ↓
     Difficulty       Packages       Local BMS
       Tables                         Files
                         │
                         ↓
                  ┌─────────────┐
                  │             │
                LR2         beatoraja
                  │             │
                  └──────┬──────┘
                         ↓
                    BMS Players
```

Library가 플레이어를 대체하는 것이 아니라, **플레이어 위/옆에 있는 컬렉션 관리 계층**이 된다.

---

# 12. 가장 중요한 차별화 기능 후보

초기 제품에서 모든 기능을 만들 필요는 없다.

다음 5개를 우선 검증한다.

### 1. Virtual Library

여러 폴더에 흩어진 BMS를 하나의 컬렉션으로 보여준다.

### 2. Content Identity

파일 경로가 달라도 같은 chart/resource를 식별한다.

### 3. Collection Health

중복·깨짐·누락·미확인 출처·오래된 버전을 보여준다.

### 4. Change Preview

패키지를 적용하기 전에 내 컬렉션에 어떤 변화가 생기는지 보여준다.

### 5. Safe Cleanup

삭제/정리 작업의 영향 범위를 계산하고 안전하게 수행한다.

이 중에서도 **Virtual Library + Content Identity + Change Preview**가 제품의 중심축이다.

---

# 13. 하지 않을 것

초기에는 다음을 피한다.

- 새로운 BMS 플레이어 만들기
- 기존 플레이어를 대체하기
- 새로운 difficulty table 생태계를 강제로 만들기
- 기존 파일을 자동으로 재구성하기
- Package 사용을 강제하기
- 모든 BMS 사이트를 하나의 다운로드 서비스로 통합하기
- BMSeeker의 모든 기능을 복제하기

목표는 **생태계를 바꾸는 것이 아니라 생태계에 접속하는 새로운 관리 계층을 제공하는 것**이다.

---

# 14. 가장 중요한 제품 가설

이 프로젝트가 검증해야 할 질문은 하나다.

> **BMS 유저가 "곡을 다운로드하는 도구"보다 "내가 가진 BMS를 이해하고 관리하는 도구"에 지속적인 가치를 느끼는가?**

이를 검증하는 가장 작은 제품은 Package Registry가 아니다.

```text
폴더 여러 개 지정
       ↓
      Scan
       ↓
Virtual Library 생성
       ↓
중복 / 누락 / 오류 / 출처 표시
       ↓
Difficulty Table과 비교
```

여기서 사용자가 실제로:

> "이거 없으니까 내 BMS 관리가 불편했네."

라고 느끼는지가 첫 번째 검증 포인트다.

---

# 15. 개발 로드맵

## Phase 0 — Research

- BMSeeker 최신 fork 기능 조사
- 주요 BMS 관리 도구 조사
- 실제 사용자 workflow 조사
- 기존 BMS 폴더 구조 조사
- difficulty table 데이터 구조 조사

**산출물:** 경쟁/사용자 workflow matrix

---

## Phase 1 — Library Core

- Scanner
- Parser
- Hash / Identity
- SQLite Library
- Incremental Scan

**목표:** 기존 BMS 컬렉션을 안전하게 모델링

---

## Phase 2 — Library UX

- Search
- Filter
- Song/Chart view
- Duplicate
- Broken/Missing Resource
- Library Health

**목표:** 실제 개인용 관리 도구로 사용할 수 있는 수준

---

## Phase 3 — Table Integration

- Difficulty Table import
- Chart matching
- Owned/Missing/Partial
- Coverage

**목표:** Library의 실용성 확대

---

## Phase 4 — Safe Operations

- Cleanup Preview
- Move
- Delete
- Merge
- Undo / Operation Log

**목표:** Library에서 실제 관리 작업 수행

---

## Phase 5 — Package Integration

- Package identity
- Version
- Provenance
- Diff
- Change Set
- Incremental update

**목표:** 기존 BMS Package 설계를 Library에 연결

---

## Phase 6 — Ecosystem

- Package registry
- Publishing
- Mirrors
- Federation
- Creator workflow

**목표:** 기존 BMS 생태계에 새로운 배포 계층을 추가

---

# 16. 성공 기준

초기 성공은 다운로드 수가 아니다.

다음 행동이 반복되는지가 중요하다.

1. 사용자가 Library를 설치한다.
2. 기존 BMS 폴더를 등록한다.
3. Scan한다.
4. "내 컬렉션의 상태"를 확인한다.
5. 중복/오류/누락을 발견한다.
6. Difficulty Table과 비교한다.
7. 이후에도 Library를 다시 실행한다.

특히 **이미 잘 정리되어 있는 사용자가 아니라, 수년간 BMS를 모아온 사용자의 컬렉션을 얼마나 잘 이해해주는가**가 중요한 테스트 대상이다.

---

# 17. 결론

BMS Package의 기술적 완성도를 높이는 것만으로는 기존 생태계에 들어가기 어렵다.

더 낮은 진입장벽은:

> **기존 BMS를 그대로 받아들이고 → 하나의 Library로 보여주고 → 컬렉션의 문제를 발견하게 하고 → 그 다음 Package/Sync를 제공하는 것**

이다.

따라서 제품의 출발점은 `BMS Package Manager`보다 **`BMS Library`**가 적합하다.

Package 시스템은 버리는 것이 아니라, Library가 충분한 가치를 가진 뒤 **컬렉션의 provenance와 synchronization을 담당하는 기반 계층**으로 재배치한다.
