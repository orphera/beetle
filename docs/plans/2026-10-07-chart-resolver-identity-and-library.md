# Chart Resolver · Chart Identity · BMS Library 로드맵 (2026-10-07)

## 배경

세 갈래의 검토 결과를 하나로 합친 계획이다.

1. 플레이어 gap 분석 (P0–P2 분류)
2. 생태계 연결고리 검증 계획 (Collection → Artifact → Manifest → Diff)
3. BMS Library 제품 전략 (개인용 컬렉션 관리자로 진입, Package는 후속)

세 갈래가 모두 같은 선행 과제에 의존한다.

> BMS 파일을 "재생 가능한 구체적 Chart"로 확정하는 계층(Resolver)과,
> 그것을 "동일한 Chart"로 식별하는 계층(Identity).

## 코드 대조 결과 (2026-10-07 기준)

| 주장 | 결과 |
|---|---|
| `#RANDOM/#IF/#ELSE/#ENDIF` 미지원 | **확인됨.** 파서에 없음 (`RANDOM`은 플레이 modifier뿐) |
| 차트 해시가 FNV-1a 64bit | **확인됨.** `compute_chart_hash` (`crates/beetle-core/src/library.rs`) |
| 판정창이 `#RANK`에 고정 | **확인됨.** `JudgeWindow::from_rank` (`crates/beetle-core/src/judge/mod.rs`) |
| Landmine이 일반 노트처럼 판정됨 | **부정확.** `NoteType::Landmine` enum과 modifier match만 있고 파서가 생성하는 곳이 없다. 판정 오류가 아니라 **지뢰 채널 파싱 자체가 없을 가능성**이 높다 (착수 전 확인 필요) |
| 9K/10K/14K 구현 완료 | 기능은 구현됨. 남은 일은 실차트 검증 |

## 코퍼스 조사 결과 (2026-10-07, `crates/beetle-core/examples/corpus.rs`)

대상은 로컬 컬렉션 약 735개 차트다 (D:\BMS 폴더 149개, 압축 패키지에서 꺼낸 차트 573개, 구형 BMS 13개, 서로 일부 겹침). 파싱 실패는 0건이다. 같은 곡이 폴더와 패키지에 중복으로 들어 있고, 대부분 최근 난이도표 위주 곡이라는 편향이 있다.

| 항목 | 결과 |
|---|---|
| `#RANDOM` 계열 | **0건** |
| 지뢰 채널(D1~D9) | 8개 (1.4%, 모두 GdbG 기믹 패키지) |
| 극단 BPM (<10 또는 >1000) | 51개 (8.9%, 대부분 sasakure.UK Jack-the-Ripper. 최대 BPM 9,990,176) |
| `#LNMODE` (beatoraja 확장) | 25개 (4.4%) |
| 실제 LNTYPE 2 채널 롱노트 | 0건 |
| 5x/6x 채널 롱노트 / LNOBJ 롱노트 | 226개 / 92개 |
| 스크롤(`#SCROLLxx`, 채널 SC) | 1개 |
| `#BACKBMP` | 125개 (한 패키지에 몰림) |
| 모드 | 7K 73%, 14K 14%, 5K 5%, 9K 7%, 10K 0.5% |

해석:

- 이 컬렉션에서는 `#RANDOM`이 나오지 않아 Resolver의 실수요를 아직 증명하지 못했다. 구형 아카이브(BOF 등 이벤트 패키지)로 다시 확인하기 전에는 Phase A의 첫 번째 항목으로 두지 않는다.
- 지뢰는 실제로 쓰이고, 현재 파서가 완전히 무시한다. `#LNMODE`와 스크롤은 쓰이지만 소수다.
- 극단 BPM은 흔하다. 박자 기준 스크롤과 STOP 처리를 이 곡들로 계속 검증한다.
- LNTYPE 2는 수요가 없어 보류한다.

## 우선순위 조정

원 분석 대비 변경점:

- **코퍼스(P0-2)를 `#RANDOM`(P0-1)보다 먼저** 만든다. 어느 gap이 실제로 아픈지 데이터로 정한다.
- **Ruleset 추상화는 P0에서 P1로 강등.** 두 번째 ruleset이 실제로 필요해질 때까지 `JudgeWindow` 확장으로 충분하다.
- **ScoreKey에 mode/modifier/ruleset 반영은 P0 유지.** 저장 포맷 마이그레이션이 걸려 있어 늦출수록 비싸다. Mirror와 정규 기록이 섞이는 문제는 지금도 발생한다.
- **LNTYPE 2 계열**은 코퍼스에서 수요를 확인한 뒤 착수한다.
- Landmine은 "판정 수정"이 아니라 "파서 지원 → 판정 → 게이지 영향" 3단계로 본다.

## 단계

### Phase A — Resolver와 코퍼스 (P0)

1. BMS 호환성 코퍼스 구성 (`parse → resolve → play chart` 자동 검증). 확장자 BMS/BME/BML/PMS, 구형 구문, 비표준 표기, 극단 BPM/STOP, LN 변형, DP/PMS, 깨진 파일, `#RANDOM`.
2. `#RANDOM/#IF/#ELSE/#ENDIF` Chart Resolver. 파서와 `BmsChart` 사이에 둔다. 시드 주입으로 결정론을 유지하고, 사용한 시드를 Replay/Score에 기록한다.
3. Landmine 채널 파싱 → 판정(gauge 감소, combo 영향).
4. LN semantics 정리 (코퍼스 결과에 따라 범위 결정).
5. ScoreKey 확장 (`chart_id × mode × modifier × ruleset`). 기존 ScoreStore 마이그레이션 포함.

### Phase B — Chart Identity (P0)

정체성 규칙을 먼저 문서(spec)로 고정하고 구현한다.

- 정규화 버전(`canonicalization version`)을 identity에 포함한다.
- 결정할 것: 암호학적 해시로 올릴지 (FNV-1a 64bit 충돌 허용 범위), 공백·개행·인코딩 차이 처리, `#RANDOM` 포함 차트의 identity (원본 소스 identity와 resolve 결과 identity를 구분).
- 같은 차트를 다른 패키지/경로에서 설치해도 같은 chart로 인식해야 한다.
- 구성: content hash, package identity, difficulty identity, provenance.

### Phase C — 읽기 전용 스캐너 (연결고리 실험 1·2)

- `bpm` 하위 명령으로 `scan/inspect`(수정 없음). Phase B의 identity를 사용한다.
- 본인의 실제 컬렉션 한 개를 대상으로 hash 안정성, 메타데이터 추출, resource 의존성 추적, 패키지 간 동일 Artifact 판정을 검증한다.
- 공개 archive 5~10개를 자동 manifest로 변환하고 **실패 지점을 기록**한다. 이 기록이 Protocol 설계 입력이다.
- 성공 기준은 정량으로 둔다 (예: archive 10개 중 N개가 사람 개입 없이 변환).

### Phase D — Library / Diff 판단 (보류)

Phase C 결과를 보고 결정한다.

- Library GUI(Health, Duplicate, 난이도표 Owned/Missing) 방향인가
- Provider/manifest 프로토콜 방향인가

두 문서의 MVP 범위가 다르므로("CLI + Diff" vs "GUI + DB") 하나로 합쳐 "Scan → Identity → Health → 난이도표 비교"를 MVP로 고정한다.

## 열린 질문 / 리스크

- **Provider 문제**: Diff의 "어디서 받을 수 있는가"는 Provider manifest 없이는 계산할 수 없다. 기존 배포물을 Provider로 해석하는 방식은 archive를 받아본 뒤에야 manifest가 생긴다. 난이도표의 URL 필드와 외부 DB 매핑을 현실적 Provider 후보로 검토한다.
- **저장소 제약**: Library 전략은 SQLite를 가정하지만 AGENTS.md는 `sqlite`/`rusqlite`를 금지한다. Beetle 본체에는 넣지 않는다. 별도 바이너리로 분리할지, 평면 파일 포맷으로 수만 chart의 incremental scan이 가능한지 먼저 검증한다.
- **Duplicate 오탐**: "Potentially Same Song"은 삭제 기능과 연결되기 전까지 표시 전용, 판정 기준은 보수적으로 둔다.
- **Package Manager와 Player의 경계**: Player는 package/diff discovery를 직접 하지 않는다. Chart Resolver는 Package Manager가 제공한 chart와 resource를 소비한다.

## 이후 (P1/P2 요약)

- P1: Ruleset 추상화, Score history, Replay metadata, Practice 고도화, Modifier(DP/PMS) 완성, 9K/10K/14K 실차트 검증, Chart Browser 그룹핑, Difficulty Table.
- P2: Course/Dan, IR/Rival, LR2·beatoraja score import, Presentation/Skin API, BGA·Audio 고급 기능, Profile.
