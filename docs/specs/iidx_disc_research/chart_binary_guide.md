# IIDX 채보 바이너리 읽기 가이드

목표: IIDX PS2 디스크(또는 추출한 파일)의 채보를 읽어서, 플레이어가 쓰는 노트 데이터로 바꾼다. BMS 파일 없이 바로 재생하는 것이 최종 목표다.

이 문서는 `<iidx-ps2tools>`(charttool, common.py, plugins)와 실제 추출 파일을 확인한 결과다. 확인한 것과 가설을 구분해서 적었다.

---

## 1. 요약

- 채보 파일은 **`.ply`**이다. (`ANISAKIS [SP NORMAL].ply`처럼 난이도 이름이 붙는다.)
- 포맷은 **7th 스타일 PS2 채보**(`charttool/plugins/iidx7thcs.py`의 `chart_to_json`)와 같다.
- ANISAKIS SP NORMAL을 이 포맷으로 읽었더니 `note_p1` 634개가 나왔다. 같은 난이도 BMS의 1P 노트 수는 631개다. **노트 개수 차이는 3개**이고, 원인은 아직 확인하지 못했다.
- 디스크의 `.ply`는 **Blowfish로 암호화**되어 있다(EMPRESS 플러그인의 `generate_encryption_key`). 추출 도구는 읽을 때 복호화한다. 이 부분을 플레이어에서 직접 읽으려면 키 생성도 옮겨야 한다.

---

## 2. 참고 코드 위치

| 경로 | 역할 |
|---|---|
| `<iidx-ps2tools>\charttool\plugins\iidx7thcs.py` | PS2 채보 읽기 `chart_to_json` (파일 헤더, 이벤트 루프) |
| `<iidx-ps2tools>\charttool\plugins\iidx16thcs.py` | 16th는 7th 로직을 그대로 호출 |
| `<iidx-ps2tools>\charttool\iidx_common.py` | `COMMAND_MAPPING`, `create_event_ps2`(이벤트 해석), `ac_parse_file`(AC용 헤더, PS2에는 쓰지 않음) |
| `<iidx-ps2tools>\common.py` | `decrypt_blowfish`, `extract_file`(암호 해제 후 저장), `extract_songs`(`.ply`를 난이도별로 모음) |
| `<iidx-ps2tools>\plugins\iidx16thempresscs.py` | EMPRESS 곡 테이블, `generate_encryption_key`, `.ply` 참조 연결 |
| `<iidx-ps2tools>\plugins\iidx16thpremiumbestcs.py` | PREMIUM BEST(Disc2). 곡 테이블은 EMPRESS 핸들러를 가져다 씀 |
| `<Scharfrichter>\...\Charts\BeatmaniaIIDXCSNew.cs` 등 | 다른 세대(AC/CS)의 채보 포맷. PS2 참고용 |

### 2.1 채보 추출 도구의 실제 흐름

이 도구는 **두 단계**다. 1단계는 디스크에서 `.ply`를 뽑는 것이고, 2단계는 `.ply`를 JSON으로 바꾸는 것이다.

**1단계: 디스크 → `.ply` 파일** (`plugins/iidx16thempresscs.py`의 `extract`, `common.py`의 `extract_files`/`extract_file`)
1. 곡 테이블(`read_songlist`)로 곡 번호, 제목, 난이도 → 파일 이름을 연결한다.
2. DATA1 항목마다 `references`(`title`, `song_id`, `filename`)를 붙인다.
3. `extract_files`가 각 항목을 `extract_file`로 저장한다. `encryption`이 있으면 `decrypt_blowfish`로 먼저 풀고 저장한다.
4. 결과: `출력\<제목> [<id>]\<제목> [SP NORMAL].ply` 같은 파일이 생긴다.

**2단계: `.ply` → JSON** (`common.extract_songs` → `iidxtool.process_file`)
1. `extract_songs`는 `.ply` 파일 이름에 `[SP NORMAL]` 같은 문자열이 있는지로 난이도를 고른다.
2. 곡마다 `input_sp_normal` 같은 키에 파일 경로를 채운다. `metadata.json`(곡 정보)도 같이 만든다.
3. `iidxtool.process_file(...)`를 호출한다. 입력 형식은 `chart_format`(예: `16thcs`)이고, 출력 형식은 `json`이다.
4. `process_file`은 입력 플러그인의 `to_json`으로 JSON 문자열을 만든 다음 출력 플러그인의 `to_chart`에 넘긴다.

**JSON 구조** (`iidx7thcs.Iidx7thCsFormat.to_json`)
```
{
  "metadata": { "version": "7thcs", "chartid": "", "title": "", "artist": "", "genre": "", "difficulty": "" },
  "charts": [
    { "chart_type": "SP NORMAL", "events": { "<시간>": [ {event}, ... ], ... } }
  ]
}
```
- `events`는 시간(`offset * timestamp_multiplier / 1000`)을 키로 하는 딕셔너리다. 같은 시각의 이벤트가 리스트로 묶인다.
- `chart_type` 값은 `params['input_charts']`의 키(`SP NORMAL` 등)에서 온다.

**주의할 점 (코드 확인 결과)**
- `iidx7thcs.to_chart`는 `NotImplemented`다. 즉 **`.ply`를 다시 쓰는 기능은 없다.** 읽기만 된다.
- `iidxtool.py`의 `__main__` 블록은 `sys`를 import하지 않았다. 직접 실행하면 `NameError`가 난다. (함수 호출로 쓰면 문제없다.)
- `iidxtool.main`의 폴더 검색은 `glob(... + "\\*.ply")`로 Windows 경로 구분자를 쓴다. 리눅스에서는 동작하지 않는다.
- `iidx16thcs.py`, `iidx9thcs.py` 등은 모두 `Iidx7thCsFormat`을 그대로 호출하는 얇은 래퍼다. 따라서 PS2 `.ply` 해석 로직은 `iidx7thcs.py` 하나에만 있다.
- `ac_parse_file`(`iidx_common.py`)의 `0x60` 헤더는 AC(아케이드) 채보용이다. PS2 `.ply`에는 쓰이지 않는다. 이 점은 이전 버전 가이드의 H1 가설이 PS2에는 해당하지 않는다는 뜻이다.

---

## 3. 암호화와 파일 연결 (EMPRESS/PREMIUM BEST)

### 3.1 파일 참조

- `plugins/iidx16thempresscs.py`는 DATA1 항목마다 `references`를 만든다.
- `.ply` 참조는 `"%s [%s].ply" % (title, difficulty)` 형식이다. 난이도 이름은 `common.DIFFICULTY_MAPPING`으로 바꾼다.
- 각 `.ply` 항목에 `encryption`(키 문자열)이 붙는다.

### 3.2 키 생성 (`generate_encryption_key`)

키는 아래 문자열들에서 **특정 위치의 글자를 골라 이어 붙인다**. 전체 코드는 `iidx16thempresscs.py`에 있다. 앞부분은 다음과 같다.

```
key_parts[7][10], key_parts[8][18], key_parts[5][10], key_parts[0][3], key_parts[7][2], key_parts[8][7], 'w', key_parts[2][4], key_parts[5][4], key_parts[3][8], ...
```

- `key_parts`는 게임 안의 고정 문자열 10개다. (예: "PERFECT FULL COMBO HARD EASY", "Beginner Tutorial", ...)
- 나머지 글자 조합은 파일의 끝까지 이어진다. 끝까지 읽지 않았다.

### 3.3 복호화

- `common.decrypt_blowfish(data, key)`
  - 데이터가 8의 배수면 0 패딩 8바이트를 붙인다.
  - `blowfish.Cipher(key.encode('ascii'), byte_order="little")`
  - `decrypt_cbc_cts(data, IV=8바이트 0)`
- 복호화 결과가 채보 본문이다.

### 3.4 확인 방법

- 디스크 원본에서 `.ply` 항목을 읽고 `decrypt_blowfish`를 거친 다음, 아래 4절의 헤더가 맞는지 확인한다.
- 헤더가 깨지면 키 문자열 조합부터 다시 본다.

---

## 4. 채보 파일 구조 (7th 스타일, `chart_to_json` 기준)

### 4.1 파일 헤더 (8바이트)

```
u32  chart_offset            ; 첫 이벤트 위치 (실제 값: 8)
if chart_offset != 4:
    u32 timestamp_multiplier ; 시간 배율 (실제 값: 0x412B)
else:
    timestamp_multiplier = 0x414C
```

- ANISAKIS SP NORMAL: `chart_offset = 8`, `timestamp_multiplier = 0x412B`

### 4.2 이벤트 (8바이트 단위)

```
u32  offset    ; 시간(틱). 0x7FFF면 종료
u8   c1        ; 하위 4bit = 명령, 상위 4bit = 파라미터(슬롯/플레이어)
u8   c2
u16  param
```

- 시간: `offset * timestamp_multiplier / 1000`
- `create_event_ps2(offset, c1, c2, param)`가 아래 표로 해석한다.

| 명령 | 이름 | 상위 니블 | 필드 |
|---|---|---|---|
| 0x0 | note_p1 | 슬롯(레인) | 1P 노트 |
| 0x1 | note_p2 | 슬롯 | 2P 노트 |
| 0x2 | sample_p1 | 슬롯 | `sound_id = param` |
| 0x3 | sample_p2 | 슬롯 | `sound_id = param` |
| 0x4 | bpm | misc | `bpm = c2` |
| 0x5 | timesig | - | `numerator = c2`, `denominator = param` |
| 0x6 | end | - | 플레이어 종료 |
| 0x7 | auto | 슬롯 | `sound_id = param` (자동 연주) |
| 0x8 | timing | 슬롯 | `judgement = c2`(signed) |
| 0x0B | unk | - | 미해석 |
| 0x0C | measure | - | 마디 구분 |
| 0x10 | notes | - | 노트 수(쓰기용) |

### 4.3 ANISAKIS SP NORMAL 실측

| 명령 | 개수 |
|---|---|
| auto | 2030 |
| note_p1 | 634 |
| sample_p1 | 469 |
| measure | 102 |
| timing | 6 |
| bpm | 2 |
| timesig | 1 |
| end | 1 |

- 파싱은 `end` 뒤 4바이트가 남은 상태로 끝났다. **`0x7FFF` 종료 마커는 나오지 않았다.** `end` 명령(0x06)으로 끝나는 것으로 보인다.
- 같은 곡 `@ANISAKIS [1P Normal].bms`의 1P 채널(11~19) 노트 수: 631개.

---

## 5. 검증 결과와 남은 차이

- **노트 수**: `note_p1` 634 vs BMS 631. 차이 3개.
  - 가설 A: 롱노트 끝 처리 등으로 같은 노트가 두 번 기록됨
  - 가설 B: BMS에서 채널 17(스크래치)이나 다른 채널이 빠짐
  - 가설 C: `auto` 이벤트 중 일부가 실제 노트
- **`auto` 2030개**: BMS에 대응 노트가 없다. 자동 연주/BGM 트리거로 보인다. 플레이어에서 판정 노트로 쓰면 안 된다.
- **시간 값**: 아직 BMS의 시각(마디 + BPM 기반)과 대조하지 않았다. `offset * 0x412B / 1000`이 실제 시간인지 확인이 필요하다.

---

## 6. 플레이어 이식 설계 (제안)

```
디스크 ISO → (ISO9660) → DATA1 컨테이너 → (항목 선택, Blowfish 복호화) → .ply 바이트
   → (7th 파서) → 이벤트 목록 → (노트만 추출, 시간 변환) → 플레이어 노트 구조
```

- 파서는 독립 모듈로 두고, 출력은 BMS 파서가 만드는 노트 구조와 같은 형식으로 맞춘다. 그러면 판정/게이지/점수 코드를 그대로 쓸 수 있다.
- 1차 목표는 **노트(`note_p1`)와 BPM(`bpm`)만** 쓰는 것이다. `sample`/`auto`는 음원 재생용으로 나중에 붙인다.

---

## 7. 내일 할 일 (순서)

1. **헤더 검증**: 추출된 `.ply`는 이미 복호화되어 있다. 디스크 원본 항목에서 같은 결과가 나오는지 확인한다. (키 생성 코드 검증)
2. **노트 수 차이 3개 원인**: BMS 채널 17, 롱노트, `auto` 이벤트를 나눠서 비교한다.
3. **시간 대조**: BPM 이벤트와 measure 이벤트로 `offset`을 마디/초로 바꿔 BMS 노트 시각과 맞춘다.
4. **파서 모듈 초안**: 노트와 BPM만 읽는 최소 버전을 만든다.

---

## 8. 확인하지 않은 것 / 리스크

- `ANISAKIS.1`(곡 폴더의 파일)의 역할. 헤더가 `(offset, size)` 쌍 12개처럼 보이지만, 채보인지 확인하지 않았다. `.ply`가 채보라는 점은 확인했다.
- `generate_encryption_key`의 나머지 글자 조합(끝까지).
- PREMIUM BEST와 EMPRESS의 `.ply` 포맷이 동일한지(같은 7th 로직을 쓰는 것으로 보이지만 두 디스크 모두 검증하지 않았다).
- 판정 창과 게이지 공식은 플레이어 쪽에서 맞춰야 한다.
- 추출한 채보/음원은 개인 용도로만 쓰고 재배포하지 않는다.
