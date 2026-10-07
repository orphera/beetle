# 점수 기록(ScoreStore) 확장 설계 (2026-10-07)

[chart resolver 계획](2026-10-07-chart-resolver-identity-and-library.md)의 Phase A 5번 ("ScoreKey에 ruleset/mode/modifier 반영")을 구체화한다. 구현 전 설계이며, 이 문서의 결정은 아래 "확정된 결정"에 적은 세 가지가 사용자 선택이고 나머지는 거기서 따라 나온 제안이다.

## 현재 상태와 문제

`crates/beetle-core/src/score.rs`의 `ScoreStore`는 `chart_hash(u64) → ScoreRecord` 하나다. 저장은 `scores.dat`의 위치 기반 TSV 한 줄이다.

| 문제 | 설명 |
|---|---|
| 기록 전체 교체 | `update`가 EX 점수 **또는** 램프가 더 높으면 기록 전체를 새 플레이로 덮어쓴다. 램프만 오른 플레이가 이전 최고 EX 점수를 지울 수 있다. |
| 램프가 게이지를 구분하지 않음 | `ClearType`은 Failed / Clear / FullCombo / Perfect뿐이다. Easy 게이지로 깨도 Hard 게이지로 깨도 같은 `CLEARED`다. |
| 플레이 조건이 남지 않음 | 모디파이어, 게이지, `#RANDOM` 시드, 엔진 버전이 기록에 없다. Mirror와 Random으로 낸 점수를 구별할 수 없다. |
| 재생 근거가 없음 | 최고 EX를 낸 플레이가 어떤 조건이었는지 알 수 없다. |
| 포맷이 고정 | 열 위치로만 읽어서, 필드를 추가하려면 모든 줄의 해석을 바꿔야 한다. |
| 비원자적 저장 | `fs::write`로 바로 덮어써서 쓰는 도중 종료되면 파일이 깨진다. |

## 확정된 결정 (사용자 선택)

1. **모디파이어**: 기록은 차트당 하나다. 모디파이어는 키가 아니라 기록에 표시만 한다 (LR2·beatoraja와 같은 방식).
2. **게이지**: 클리어 램프 단계에 게이지 종류를 넣는다. 기록은 여전히 차트당 하나다.
3. **갱신**: 항목별로 각자 최고값을 유지한다 (EX 점수, 램프, 최대 콤보, 최소 BP).

## 키는 바꾸지 않는다

처음에는 `chart × ruleset × mode × modifier`를 키로 하려 했지만, 위 결정에 따라 키는 **차트 하나**로 남는다.

- **mode**는 차트 내용에서 정해지므로 키에 따로 넣을 이유가 없다.
- **modifier**와 **gauge**는 키가 아니라 기록 안의 정보다.
- **ruleset**은 아직 하나뿐이라 키가 아니라 `engine` 버전 필드로 남긴다. 판정 규칙이 달라지는 시점에 이전 기록과 구분하는 용도다.
- 키 자체는 Chart Identity(Phase B)에서 바뀐다. 파일에는 키를 **불투명한 문자열 필드** (`chart=`)로 저장해서, 지금은 16자리 16진 해시이고 나중에 정규화 버전이 붙은 식별자가 와도 포맷을 바꾸지 않게 한다. 메모리 안의 키 타입은 지금은 `u64`로 두고, Identity 작업 때 함께 바꾼다.

## 클리어 램프 단계

`ClearType`을 제자리에서 확장한다. 선언 순서가 곧 대소 순서(`Ord`)다.

```text
Failed < Easy < Clear < Hard < FullCombo < Perfect
```

- `Clear`는 Groove 게이지 클리어다. 기존 기록의 `C`가 그대로 이 단계로 올라간다.
- `FullCombo`와 `Perfect`는 게이지와 무관하다 (BAD, POOR, MISS가 없으면 어떤 게이지로든 FullCombo 이상). Easy 게이지의 FullCombo는 Hard 클리어보다 높은 램프다.
- **Hazard 게이지에는 램프를 따로 두지 않는다.** Hazard는 BAD, POOR, MISS 하나만 나와도 실패하므로, Hazard 클리어는 항상 `FullCombo` 이상이다. 처음 설계에 있던 `Hazard` 단계는 도달할 수 없어서 구현 중에 뺐다. 어떤 게이지로 깼는지는 기록의 `gauge` 필드가 남긴다.
- 화면 표시는 `EASY CLEAR`, `CLEAR`, `HARD CLEAR`, `FULL COMBO`, `PERFECT`로 한다. 곡 목록 램프 색, 정렬(`ClearLamp`), 분류(`BY CLEAR STATUS`)가 모두 이 순서를 따른다.

## 기록 한 건의 내용

```text
ScoreRecord
 ├─ chart            키 (현재 16자리 16진 해시)
 ├─ 항목별 최고값 (각자 독립)
 │   ├─ lamp         ClearType 최고
 │   ├─ ex_score     최고 EX 점수
 │   ├─ max_combo    최대 콤보
 │   └─ min_bp       최소 BAD+POOR+MISS
 ├─ 최고 EX 플레이의 스냅샷 (EX가 갱신될 때만 통째로 교체)
 │   ├─ pgreat, great, good, bad, poor, miss
 │   ├─ total_notes  그 플레이의 총 노트 수 (정확도 계산 기준)
 │   ├─ modifier     Regular / Mirror / Random / R-Random / S-Random
 │   ├─ gauge        Easy / Groove / Hard / Hazard
 │   └─ random_seed  `#RANDOM` 차트일 때만
 ├─ 이력
 │   ├─ play_count   수동 완주 플레이 횟수
 │   ├─ clear_count  클리어한 횟수
 │   └─ last_played  마지막 플레이 시각 (유닉스 초)
 └─ engine          판정 규칙 버전 (1부터. 이전 기록은 0)
```

- 정확도(`accuracy_rate`)는 저장하지 않고 `ex_score / (total_notes × 2)`로 계산한다. 저장된 값과 어긋날 일이 없다.
- 랭크(AAA 등)도 최고 EX 스냅샷에서 계산한다.
- 저스트 판정 오프셋과 하이스피드는 점수의 의미를 바꾸지 않으므로 기록하지 않는다.

## 갱신 규칙

`ScoreStore::update(play)`는 플레이 결과 하나를 받아 항목별로 비교한다.

| 항목 | 갱신 조건 |
|---|---|
| lamp | 새 램프가 더 높다 |
| ex_score | 새 EX가 더 높다. 이때 스냅샷 전체(판정 수, total_notes, modifier, gauge, seed)를 교체한다 |
| max_combo | 새 콤보가 더 높다 |
| min_bp | 새 BP가 더 낮다 |
| play_count, clear_count, last_played | 매번 갱신한다 |

반환값은 무엇이 올랐는지 알려주는 구조체다 (`ScoreUpdate { lamp, ex, combo, bp }`, `any()`). 결과 화면은 이것으로 항목별 "NEW RECORD"를 표시한다. 현재 `is_new_record: bool`은 `any()`로 대체한다.

**기록하지 않는 플레이**는 지금과 같다: 오토 플레이, 리플레이 재생, 연습 모드(시작 마디가 0이 아님).

## 파일 형식과 이전

- 새 형식은 첫 줄이 `#BEETLE_SCORES_V2`이고, 줄마다 `필드=값`을 탭으로 이어 쓴다.

  ```text
  #BEETLE_SCORES_V2
  chart=0123456789abcdef	lamp=H	ex=1520	combo=850	bp=3	n=1688	pg=700	gr=140	gd=2	bd=1	pr=0	ms=2	mod=MIRROR	gauge=HARD	plays=12	clears=7	last=1791500000	engine=1
  ```

  모르는 필드는 읽을 때 무시하므로, 이후 필드를 추가해도 호환된다. 줄 순서와 필드 순서는 고정해서 같은 내용은 같은 바이트가 되게 한다 (HashMap 순회 순서에 의존하지 않는다: 키 순으로 정렬해서 쓴다).
- 첫 줄에 헤더가 없으면 옛 형식(`scores.dat` v1)으로 읽는다. 옛 기록은 다음과 같이 채운다.
  - lamp: `C` → `Clear`, 나머지는 같은 의미
  - `min_bp`: 저장된 BAD+POOR+MISS
  - `n`(total_notes): 알 수 없음 → 비워 둔다
  - `mod`, `gauge`: 알 수 없음 → 비워 둔다 (화면에 표시하지 않음)
  - `plays`: 1, `engine`: 0
- **백업**: v2로 처음 저장하기 전에 기존 파일을 `scores.dat.v1.bak`으로 복사한다 (이미 있으면 덮어쓰지 않는다).
- **원자적 저장**: 임시 파일에 쓴 뒤 이름을 바꾼다.
- 리플레이 파일은 지금처럼 차트당 하나(`<해시>.rep`)이고, **EX 점수가 갱신될 때** 저장한다 (지금은 "기록이 올랐거나 파일이 없을 때"). 리플레이 헤더에 `modifier=`, `gauge=`를 추가한다 (재생 때 같은 조건을 쓰는 것은 이번 범위 밖).

## 알려진 위험

- **LN 정확도가 부풀려진 옛 기록**: 롱노트 꼬리를 총 노트 수에 넣도록 고치기 전의 기록은 롱노트 차트에서 EX 점수가 최대치를 넘을 수 있다. 옛 기록에는 `total_notes`가 없어 보정할 수 없다. 옛 기록의 정확도가 100%를 넘으면 그 EX를 신뢰하지 않고, 다음 플레이의 EX로 대체한다. 100% 이하인 부풀려진 값은 감지할 수 없고, 그대로 둔다.
- **게이지 램프 해석**: 옛 `C`는 Groove 클리어로 올렸지만 실제로는 Easy나 Hard였을 수 있다.
- **프로필 분리**: 지금은 전역 기록 하나다. 프로필(P2)이 생기면 파일 이름이나 폴더로 나눈다.

## 구현 단계

1. **core** (완료): `ClearType` 확장, `ScoreRecord` 재구성, 항목별 갱신, `ScoreUpdate`, v2 읽기/쓰기와 v1 이전, 단위 테스트. 이 단계는 앱 코드를 바꾸지 않고 `finish_gameplay`가 쓰는 호출 모양만 호환되게 유지한다.
2. **app**: `finish_gameplay`가 모디파이어, 게이지, 시드를 담아 `update`를 호출한다. 백업과 원자적 저장, 리플레이 저장 규칙 변경.
3. **render**: 새 램프 이름과 색, 결과 화면의 항목별 "NEW RECORD", 선곡 화면 개인 최고 패널의 모디파이어와 게이지 표시, 정렬과 분류.

단계 1에서 앱이 컴파일되도록 `finish_gameplay`를 `PlayResult` 기반으로 바꾸고, 백업과 원자적 저장도 함께 넣었다 (단계 2의 일부를 앞당김). 실제 `scores.dat` 사본으로 25개 기록이 모두 읽히고 다시 쓴 파일을 읽은 결과가 같은 것을 확인했다.

각 단계는 별도 커밋으로 하고, 단계 1이 끝난 시점에 이전된 실제 `scores.dat`로 읽기 검증을 한다.
