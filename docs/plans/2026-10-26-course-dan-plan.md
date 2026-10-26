# 단위인정 / 코스 플레이 기획

상태: C1~C6 구현됨 (2026-10-26). 판정 변형, 사용자 정의 코스, `.lr2crs`는 이후 과제.

## 1. 목표

- 여러 채보를 끊김 없이 이어서 플레이하는 **코스**를 지원한다.
- 코스 중 특정 규칙(게이지 이월, 옵션 제한)을 강제하는 **단위인정(段位認定)** 을 지원한다.
- 별도 편집기를 만들지 않는다. 코스 정의는 기존 난이도표(`bpm table`)가 이미 가져오는 `header.json`의 `course` 필드에서 얻는다.

## 2. 다른 재생기 조사

> 아래는 공개 문서·소스·실사용 관행 기준 정리. "확인 필요" 표시는 구현 전에 소스로 다시 볼 것.

### 2.1 원작 (beatmania IIDX)

- 4곡 연속. 게이지는 100%에서 시작해 곡 사이에 **이월**되며, 0%가 되면 즉시 불합격.
- 회복량이 작고 감소량이 고정인 단위 전용 게이지. 옵션은 제한(미러 정도만 허용).
- 합격하면 단위(7단~10단, 중전 등)를 획득. 여러 곡을 이어 치는 "체력 시험"이 핵심 재미.

### 2.2 Lunatic Rave 2 (LR2)

- 코스 파일: `LR2files/Database/` 아래 `.lr2crs` (XML). 코스명, 종류, 채보들의 MD5를 이어붙인 해시를 담음.
- 종류: 일반 코스 / 단위인정. 단위인정은 전용 게이지(HARD 계열, 곡 간 이월)와 옵션 고정. (세부 수치 확인 필요)
- 기록은 코스 해시 단위로 DB에 저장. 코스 결과 화면에서 곡별 결과와 합계를 함께 보여줌.
- 커뮤니티 단위(発狂BMS段位, 通常段位 등)는 사실상 이 `.lr2crs` 배포로 보급됨.

### 2.3 beatoraja

현재 사실상의 표준. 두 경로로 코스를 얻는다.

1. **난이도표 `header.json`의 `course` 필드** — 2차원 배열(그룹 → 코스). 코스 항목 예:
   ```json
   {
     "name": "発狂皆伝",
     "md5": ["...", "...", "...", "..."],
     "constraint": ["grade_mirror", "gauge_lr2"],
     "trophy": [
       {"name": "bronzemedal", "missrate": 7.5, "scorerate": 55.0},
       {"name": "silvermedal", "missrate": 5.0, "scorerate": 70.0},
       {"name": "goldmedal",   "missrate": 2.5, "scorerate": 85.0}
     ]
   }
   ```
   (`sha256` 배열도 허용. `md5`/`sha256` 대신 `charts` 객체 배열을 쓰는 표도 있음 — 확인 필요)
2. **로컬 코스 파일** — `table/` 혹은 `course/` 폴더의 사용자 정의 JSON. 레벨 조건으로 곡을 뽑는 랜덤 코스도 있음.

`constraint` 값 (게이지/옵션 강제):

| 값 | 의미 |
|---|---|
| `grade` | 단위인정. 배치 옵션 금지(정배치만) |
| `grade_mirror` | 단위인정. 미러까지 허용 |
| `grade_random` | 단위인정. 랜덤 허용 |
| `no_speed` | 플레이 중 하이스피드 변경 금지 |
| `no_good` / `no_great` | GOOD / GREAT 이하가 나오면 미스 처리 |
| `gauge_lr2`, `gauge_5k`, `gauge_7k`, `gauge_9k`, `gauge_24k` | 게이지 수치 체계 선택 |
| `ln` / `cn` / `hcn` | 롱노트 판정 방식 강제 |

- 단위 게이지: `CLASS` / `EXCLASS` / `EXHARDCLASS` (게이지 옵션이 EASY·NORMAL이면 CLASS, HARD면 EXCLASS, EXHARD면 EXHARDCLASS로 매핑). 곡 간 이월.
- **트로피**: 합격 후 코스 전체의 미스율(%)·스코어율(%)로 동·은·금 메달 부여.
- 기록: 코스를 구성하는 채보 해시를 이어 만든 키로 저장. 곡별 결과 + 코스 합계(EX 스코어, 미스, 클리어 램프) 표시.
- 다음 곡은 앞 곡을 치는 동안 미리 로드해서 곡 사이 대기를 줄임.

### 2.4 그 밖

- **Qwilight**: 자체 코스 기능과 단위 지원이 있으나 포맷은 독자적 (확인 필요). 호환 대상 아님.
- **Bemuse**: 웹 재생기. 코스/단위 없음.

### 2.5 시사점

- 실제 단위 데이터는 **beatoraja 표의 `course` 필드**로 대부분 유통된다 (発狂段位, Satellite/Stella, Genoside 등). 이것만 읽어도 주요 단위를 거의 다 커버.
- `.lr2crs`는 레거시. 2단계 이후 선택 지원.
- 핵심 메커니즘은 단순하다: **곡 목록 + 게이지 이월 + 옵션 제약 + 합계 결과**.

## 3. Beetle 설계

### 3.1 데이터 원천

- `bpm table update`가 `header.json`을 이미 받는다. 여기서 `course`를 읽어 표 캐시(`#BEETLE_TABLE_V1`)에 `#COURSES` 구역을 추가해 함께 저장.
  - 형식은 기존 이스케이프 필드 라인 형식 유지(`crates/beetle-core/src/escape.rs`). JSON은 `bpm` 쪽에서만 다루고 플레이어는 플랫 파일만 읽음 (의존성 정책: 핫패스 `serde_json` 금지).
  - 캐시 버전은 V1을 그대로 두고 구역만 추가 (dev 전용, 마이그레이션 UX 없음).
- 모르는 `constraint` 값은 보존만 하고 무시, 선곡에서 "미지원 규칙" 표시.

### 3.2 `beetle-core` (순수 로직)

- `course.rs` 신설
  - `CourseDef { name, charts: Vec<ChartRef>, constraints: Vec<Constraint>, trophies: Vec<Trophy> }`
  - `ChartRef`는 표 엔트리와 같은 MD5/SHA-256 매칭 재사용(`TableIndex`).
  - `CourseId` = 구성 채보 SHA-256을 순서대로 이어 `bms-hash`로 다시 해싱.
- `GaugeType`에 `Class`, `ExClass`, `ExHardClass` 추가. 수치는 beatoraja `gauge_lr2`/`gauge_7k` 기준으로 정하고 테스트로 고정.
- `CourseRun` 상태 기계: `stage`, 이월 게이지 값, 곡별 `ScoreTracker` 결과 누적, `failed` 여부.
  - 게이지 이월은 `ScoreTracker` 생성 시 초기 게이지를 받도록 확장.
  - 판정은 기존 그대로 오디오 클럭 기준 (INV-1). 곡마다 클럭을 새로 시작.

### 3.3 `beetle-app`

- 화면 흐름: `SongSelect` → (코스 선택) → `Loading`(1곡) → `Gameplay` → `Loading`/`Gameplay`(2곡…, 곡간 결과 없음) → `CourseResult`.
- **다음 곡 선로딩**: 현재 곡 플레이 중 워커 스레드에서 다음 곡 키음 디코딩 (INV-3, INV-5). 메모리 상한을 넘으면 곡 사이 로딩 화면으로 대체.
- 옵션 제약: 플레이 옵션 패널(`options_table.rs`)에서 제약된 항목을 잠금 표시. `no_speed`면 플레이 중 하이스피드 핫키 무시.
- 불합격(게이지 0)은 즉시 코스 결과로. 중도 이탈은 불합격 처리.
- 오토플레이·리플레이 중에는 기록 저장 안 함.

### 3.4 `beetle-render`

- 선곡: 표 폴더 아래 "코스" 하위 폴더. 코스 행에는 구성 곡 4개, 보유 여부(누락 곡 있으면 회색 + 시작 불가), 최고 기록·메달.
- 플레이 중 상단에 `STAGE 2/4`와 이월 게이지.
- 코스 결과: 곡별 행(EX, 미스, 게이지 잔량) + 합계, 합격/불합격, 트로피.
- 문자열은 `strings.rs`, 색은 `theme.rs` 토큰.

### 3.5 기록 저장

- `courses.dat` 플랫 파일: `CourseId` → 최고 클리어(불합격/합격/트로피), 최고 EX 합계, 최소 미스, 도달 스테이지.
- 단위 "획득 단위" 표시는 표별로 합격한 가장 높은 코스(표 그룹 순서 기준).

## 4. 결정 사항 (2026-10-26)

1. **게이지 수치는 beatoraja와 맞춘다.** CLASS / EXCLASS / EXHARDCLASS와 `gauge_lr2`·`gauge_7k` 등 체계를 beatoraja 소스 수치 그대로 옮기고 테스트로 고정한다. 기록을 beatoraja와 비교할 수 있게 하기 위함.
2. **곡 사이 결과 화면은 없다.** 곡이 끝나면 바로 다음 곡 로딩/플레이로 넘어가고, 곡별 결과는 코스 결과 화면에서만 보여준다. 그래서 C5(다음 곡 선로딩)의 중요도가 올라간다.
3. **판정 변형(`no_good`/`no_great`)은 지금 구현하지 않는다.** 대신 `Constraint`를 열거형으로 파싱·보존하고, 판정 결과를 `ScoreTracker`에 넣기 전에 거치는 변환 지점(`JudgeGrade` → `JudgeGrade`) 하나를 둬서 나중에 끼울 수 있게 한다. 지금은 해당 제약이 있는 코스에 "미지원 규칙" 표시만.
4. **사용자 정의 코스 UI도 지금 구현하지 않는다.** 대신 코스의 출처를 `CourseSource { Table(표 인덱스), Disc(디스크), Local(파일) }`로 추상화하고, `CourseDef`가 출처와 무관하게 같은 모양이도록 한다. 나중에 로컬 코스 파일(`courses/` 폴더)과 곡 메뉴의 "코스에 추가"를 붙이면 된다.
5. **CS 디스크의 원작 단위도 읽는다.** `cs-disc`에서 단위인정 구성(단위별 4곡, 게이지 규칙)을 찾아 `CourseSource::Disc`로 제공한다.
   - 먼저 조사: 단위 표가 실행 파일(ELF) 데이터 영역에 있는지, 곡 표 근처에 있는지. 찾은 내용은 `docs/specs/iidx_ps2_disc.md`에 근거 등급과 함께 기록.
   - 원작 단위 게이지는 beatoraja CLASS 게이지와 다르지만, 1번 결정에 따라 일단 CLASS 게이지로 플레이하고 원작 게이지는 별도 과제로 둔다.
   - 에디션마다 단위 구성이 다르므로 `Layout` 표에 위치를 추가하는 방식으로.

## 5. 단계

| 단계 | 내용 |
|---|---|
| C1 | 표 `course` 파싱·캐시 저장, `Constraint`·`CourseSource` 모델 |
| C2 | `CourseRun`, beatoraja 게이지 수치 이식, 판정 변환 지점(빈 구현) |
| C3 | 화면 흐름(곡간 결과 없이 바로 이어짐), 선곡 코스 폴더, 코스 결과 |
| C4 | 옵션 제약 잠금·트로피·`courses.dat` |
| C5 | 다음 곡 선로딩 |
| C6 | CS 디스크 단위 조사 → `CourseSource::Disc` |
| 이후 | 판정 변형, 사용자 정의 코스, `.lr2crs` |
