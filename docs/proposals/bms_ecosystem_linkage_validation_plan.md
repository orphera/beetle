# BMS 생태계 연결고리 검증 계획

## 1. 문제 정의

현재 BMS 생태계에는 개별적으로 잘 작동하는 여러 규약과 도구가 존재한다.

- 플레이어: LR2, beatoraja, Qwilight 등
- 난이도표: Stella, Satellite, 발광 등
- 검색/DB: BMS Search 및 여러 데이터베이스
- 컬렉션 관리: BMSeeker, BmsManager 등
- 배포: 이벤트 패키지, BMS Library, 개인 배포 페이지, torrent 등

문제는 이들을 연결하는 **상위 프로토콜이 부재**한다는 것이다.

특히 다음 관계가 일관되게 표현되지 않는다.

`Artifact → Package → Collection → Player`

그리고 사용자가 실제로 필요한 것은 단순한 파일 관리가 아니라:

> "내가 무엇을 가지고 있고, 무엇이 빠졌으며, 어떤 배포처에서 그것을 얻을 수 있는가?"

를 알아내는 것이다.

---

## 2. 기존 생태계에서 발견한 중요한 사실

### bndlr

BMS에는 이미 비슷한 선행 시도가 있었다.

- BMS Bundle Manifest
- manifest 기반 자동 설치
- 리소스 목록과 다운로드 정보 표현
- 업데이트/diff 개념 지원

하지만 공급자가 직접 manifest를 제공해야 한다는 점이 확산의 병목이 된 것으로 보인다.

### 핵심 교훈

**"새로운 프로토콜을 만들면 사람들이 사용할 것"이라고 가정하면 안 된다.**

기존 배포자에게 새로운 작업을 요구하지 않고도, 기존 배포 행위를 프로토콜의 Provider로 해석할 수 있어야 한다.

---

# 3. 제품의 핵심 가설

## Provider를 새로 모집하지 않는다.

기존의 배포물을 Provider로 만든다.

예:

- BMS Library
- 이벤트 패키지
- 제작자의 기존 다운로드 페이지
- 기존 archive URL

사용자는 기존 방식대로 파일을 다운로드한다.

우리 시스템은 그 파일을 분석하여:

`Archive → Artifact → Manifest`

로 정규화한다.

즉 제작자에게:

> "새로운 manifest 형식으로 다시 배포해주세요."

라고 요구하지 않는다.

---

# 4. 제품의 핵심 모델

## Artifact

개별 BMS / 차분 / 리소스.

가능하면 hash를 기반으로 동일성을 판단한다.

## Package

여러 Artifact의 묶음.

버전과 변경 내역(diff)을 표현할 수 있다.

## Collection

사용자의 실제 로컬 보유 상태.

예:

- 보유 Artifact
- 출처 Package
- 적용된 diff
- 난이도표 관계

## Provider

Artifact를 발견하거나 다운로드할 수 있는 외부 출처.

Provider가 반드시 파일을 직접 호스팅할 필요는 없다.

예:

`Artifact ID → Download URL`

## Manifest

Artifact, Package, Provider, dependency 등의 관계를 기계적으로 표현하는 메타데이터.

---

# 5. 가장 먼저 검증할 것

아직 Package Manager를 만들지 않는다.

첫 질문은 이것이다.

> **"BMS Collection을 정확하게 기술할 수 있는가?"**

이를 실제 컬렉션으로 검증한다.

---

# 6. 실험 1 — 실제 BMS Collection 스캔

내가 가진 BMS 폴더 하나를 대상으로 한다.

목표:

```text
BMS files
   ↓
hash
   ↓
BMS metadata
   ↓
Artifact
   ↓
Collection
```

확인할 것:

- 동일 곡/차분을 hash로 안정적으로 식별할 수 있는가?
- BMS 내부 metadata를 충분히 추출할 수 있는가?
- resource dependency를 추적할 수 있는가?
- 같은 Artifact가 서로 다른 패키지에 들어 있어도 동일하게 판단할 수 있는가?

---

# 7. 실험 2 — 기존 배포물을 역으로 분석

기존 배포처 5~10개를 선정한다.

예:

- BMS Library
- 이벤트 패키지
- 기존 BMS 다운로드 사이트
- 기타 공개 archive

각 archive를 입력으로 넣어:

```text
ZIP / archive
   ↓
파일 분석
   ↓
BMS parsing
   ↓
hash
   ↓
Artifact 목록
   ↓
자동 Manifest
```

을 시도한다.

중요한 것은 **자동화가 실패하는 지점**을 기록하는 것이다.

그 실패 지점이 향후 Protocol 설계 대상이다.

---

# 8. 실험 3 — Collection Diff

아주 작은 CLI를 만든다.

예:

```bash
bms inspect ~/BMS
bms diff ~/BMS provider.json
```

최소 출력:

```text
Collection
──────────
Total artifacts: 12,431

Missing
───────
Stella ★★7
  25 missing

Available
─────────
18 artifacts
  Provider: Example Package
  Source: example.com/...
```

목표는 **"내 컬렉션의 상태를 처음으로 한눈에 볼 수 있다"**는 경험을 검증하는 것이다.

---

# 9. MVP에서 만들지 않을 것

초기에는 다음을 만들지 않는다.

- 새로운 BMS Player
- 새로운 Difficulty Table
- 완전한 Package Manager
- 자체 BMS 호스팅 서비스
- 자체 대규모 DB
- GUI
- 모든 기존 서비스와의 통합

핵심은 **Collection Scanner + Manifest Import + Diff**다.

---

# 10. 성공 조건

다음 세 가지가 확인되면 다음 단계로 넘어간다.

### A. Collection 정규화

실제 BMS 폴더를 안정적으로 Artifact 목록으로 만들 수 있다.

### B. 기존 배포물 정규화

기존 archive에서 사람이 manifest를 작성하지 않아도 Artifact 목록을 생성할 수 있다.

### C. Diff가 유용하다

사용자가 실제로 다음 정보를 얻을 수 있다.

> "내가 가진 것"

> "내가 없는 것"

> "없는 것 중 어디서 받을 수 있는 것"

이 결과가 기존 사용자에게도 충분히 유용하다면 제품화 가치가 있다.

---

# 11. 이후의 제품 방향

검증이 성공하면 다음 구조로 확장한다.

```text
                 ┌──────────────┐
                 │   Provider   │
                 └──────┬───────┘
                        │
                   Manifest
                        │
                        ▼
┌────────────┐    ┌─────────────┐
│ Collection │ ←→ │ Diff Engine │
└─────┬──────┘    └─────────────┘
      │
      ▼
┌────────────┐
│  Artifact  │
└─────┬──────┘
      │
      ▼
┌────────────┐
│   Player   │
└────────────┘
```

장기적으로는 여러 Provider가 참여할 수 있는 구조를 목표로 한다.

---

# 12. 가장 중요한 제품 전략

핵심은 **"새로운 BMS 관리 프로그램"을 만드는 것이 아니다.**

이미 존재하는:

- 파일
- 패키지
- 난이도표
- 다운로드 사이트
- 플레이어
- 로컬 컬렉션

을 하나의 모델로 연결하는 것이다.

특히 **기존 제작자나 배포자에게 새로운 작업을 요구하지 않는 것**이 중요하다.

> 기존 배포물을 우리가 이해하고,
> 기존 사용자의 컬렉션을 우리가 이해하고,
> 둘 사이의 차이를 계산한다.

그 결과가 유용하다는 것이 먼저 증명되어야 한다.

---

# 13. 당장 할 일

### Step 1
실제 BMS 컬렉션 하나 선정.

### Step 2
Artifact identity와 metadata 추출기를 만든다.

### Step 3
공개 BMS archive 5~10개를 수집한다.

### Step 4
archive → 자동 Manifest 변환을 실험한다.

### Step 5
Collection ↔ Manifest diff CLI를 만든다.

### Step 6
실제 결과를 보고:

- 무엇이 자동으로 연결되는가?
- 어디서 identity가 깨지는가?
- 어떤 metadata가 부족한가?
- Provider 정보가 어떻게 표현되어야 하는가?

를 정리한다.

### Step 7
그때 Protocol과 Package Manager의 스펙을 설계한다.

---

## 한 문장으로

**BMS Package Manager를 만들기 전에, "BMS Collection을 하나의 데이터셋으로 읽고 기존 배포물과 비교할 수 있는가?"를 먼저 증명한다.**

이 실험이 성공하면 그 위에 Package / Provider / Manifest Protocol을 쌓는다.
