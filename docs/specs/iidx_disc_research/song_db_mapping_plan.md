# 외부 곡 DB 매핑 계획 (tts1374/iidx_all_songs_master)

목적: 디스크에서 얻은 곡(ASCII 제목, 난이도 레벨, 텍스처)을 외부 DB의 곡 정보(일본어 제목, artist, genre)와 연결한다. 디스크에는 artist와 일본어 제목이 텍스트로 없고, 이미지로만 있다.

## 0. 중요: 이 DB에는 CS(PS2) 정보가 없다

README 확인 결과(2026-10-23):
- 데이터 원천은 Textage의 `titletbl.js`, `datatbl.js`, `actbl.js`, INFINITAS 공식 페이지, AC/INF 사용자 입력 파일뿐이다.
- CS, PS2, 콘솔 관련 언급이 없다.
- `music.version`은 "수록 버전"이라고만 설명되어 있고, 값의 예시나 형식이 없다.
- 따라서 **EMPRESS/PREMIUM BEST(CS) 곡이 이 DB에 들어 있다는 근거는 없다.** AC에 수록된 곡만 있을 가능성이 크다.

Textage 페이지 `https://textage.cc/score/index.html?a001B0G0`는 브라우저로 열어서 확인했다(Shift-JIS, JS 렌더링). 제목 "[CS EMPRESS]" 목록이 보이고, 이 페이지가 쓰는 데이터 파일은 다음과 같다. 모두 Shift-JIS다.

| 파일 | 크기 | 내용 (확인됨) |
|---|---|---|
| `titletbl.js` | 약 212KB | `'키':[버전, textage_id, 옵션, GENRE, ARTIST, TITLE, SUBTITLE]`. 장르, 아티스트, 제목, 부제가 모두 있다. |
| `cstbl.js` | 약 30KB | CS 표 `cstbl[n]`. 첫 값(1, 2, ...)이 CS 버전 번호로 보인다. 형식: `'키':[버전, SP-B, SP-N, SP-H, SP-A, SP-X, DP-B, DP-N, DP-H, DP-A, DP-X, ...]` |
| `cstbl1.js`, `cstbl2.js` | 약 40KB, 57KB | 같은 형식의 다른 CS 버전 표(`cstbl[3]`, `cstbl[9]`) |
| `cltbl.js` | 약 19KB | 키별 값 표(쓰임 미확인) |
| `datatbl.js` | 약 214KB | 난이도별 노트 수 등 (쓰임 미확인) |

따라서 **CS 정보는 이 SQLite가 아니라 Textage의 JS 파일에 있다.** 이 파일들을 직접 파싱하면 아티스트·장르·일본어 제목과 CS 레벨을 얻을 수 있다. 다만 `titletbl.js`의 키(`'zero-one'` 같은 영문 식별자)와 디스크 ASCII 이름의 대응은 따로 맞춰야 한다.

Textage 페이지의 `Genre Title Artist` 열 목록은 `titletbl.js`를 불러와서 만든다. 위 파일을 받으면 SQLite 없이도 매핑을 계획할 수 있다.

### 0.1 파일 받기와 1차 매칭 결과 (2026-10-23)

- 저장 위치: `<iidx16_extract>\textage\` (`titletbl.js`, `cstbl.js`, `cstbl1.js`, `cstbl2.js`, UTF-8 변환본)
- `titletbl.js`: 2639개 항목 파싱 (페이지 주석 기준 2788개 줄 중 일부는 다른 형식)
- 1차 매칭 방식: 디스크 ASCII 이름과 `titletbl`의 제목(TITLE 필드)을 **대문자화 + 영문·숫자만 남김**으로 정규화해서 비교했다.

| 디스크 | 곡 수 | 1차 매칭 | 미매칭 |
|---|---|---|---|
| PREMIUM BEST (disc2) | 99 | 79 | 20 |
| EMPRESS (disc1) | 100 | 72 | 28 |

미매칭 예시와 원인:
- 일본어 제목이 제목 필드에 들어가 있어서 ASCII와 안 맞는 경우: `ATTACK NO3`(アタックNO.3), `HIMIKO`(卑弥呼), `TAMAYURA`(たまゆら), `KACHOFUSETSU`(華蝶風雪)
- 표기 차이: `ZERO`(零 - ZERO -), `ERA(STEP MIX)`, `FROZEN RAY (ORIGINAL MIX)`, `SCRIPTED CONNECTION N MIX`
- 같은 곡 다른 버전: `NO CRIME` / `NO CRIME J`, `THE SHINING POLARIS` / `(KORS K MIX)`

보완 계획:
1. 키(`'zero-one'` 같은 영문 식별자)도 함께 비교한다.
2. 부제(SUBTITLE 필드)와 별칭 CSV로 나머지를 맞춘다.
3. 동일 제목이 여러 개면 `cstbl`에 CS 난이도가 있는 쪽을 택하고, 레벨 집합으로 최종 확인한다.
4. 여전히 안 맞는 곡은 수동 처리한다.

### 0.2 매핑 테이블 결과 (2026-10-23, 제목 + 레벨 기준)

산출물 (`<iidx16_extract>\textage\`):

| 파일 | 내용 |
|---|---|
| `titletbl.js`, `cstbl.js`, `cstbl1.js`, `cstbl2.js`, `datatbl.js`, `cltbl.js`, `actbl.js` | Textage 원본 데이터 (UTF-8 변환본, `.raw`는 Shift-JIS 원본) |
| `map_disc2_premium.csv` | PREMIUM BEST 99곡 매핑 |
| `map_disc1_empress.csv` | EMPRESS 100곡 매핑 |
| `manual_todo_disc2_premium.csv` | PREMIUM BEST 미매칭 20곡 (수동으로 `textage_key` 채우기) |
| `manual_todo_disc1_empress.csv` | EMPRESS 미매칭 30곡 (수동으로 `textage_key` 채우기) |

결과:
- PREMIUM BEST: 79곡 자동 매칭, 전부 디스크 레벨과 Textage CS 레벨 일치. 20곡 미매칭.
- EMPRESS: 70곡 자동 매칭(제목 + 레벨 일치). 2곡은 제목만 일치하고 레벨이 다름(확인 필요). 28곡 미매칭.

매칭 규칙 메모:
- `titletbl.js`의 제목은 대문자화 + 영문·숫자만 남겨서 비교했다.
- `cstbl` 행의 레벨 순서: 쌍 `[SBo, SB, SN, SH, SA, SX, DB, DN, DH, DA, DX]`에서 SP 초보는 `SBo`와 `SB` 중 0이 아닌 값이다. 디스크 순서(SPN, SPH, SPA, SPB, SPX, DPN, DPH, DPA, DPB, DPX)로 재배열해서 비교했다.
- 같은 키가 여러 `cstbl` 파일에 나오는 경우가 있으므로, 레벨이 맞는 행을 기준으로 골랐다.
- 인코딩: `titletbl.js`는 Shift-JIS 중 `cp932`로 디코딩해야 `☆` 등이 깨지지 않는다.

### 0.4 CS 테이블 인덱스 확정 (EMPRESS = cstbl[16], PREMIUM BEST = cstbl[17])

`cstbl*.js`의 블록 번호를 확인하고, 디스크 곡 키 커버리지로 판정했다.

| 블록 | 파일 | 항목 수 | 디스크 커버리지 |
|---|---|---|---|
| `cstbl[16]` | `cstbl.js` | 99 | EMPRESS 98/98 |
| `cstbl[17]` | `cstbl.js` | 99 | PREMIUM BEST 97/97 |
| `cstbl[15]` | `cstbl.js` | 101 | 제외 (PREMIUM 일부와 겹침, 다른 버전) |

최종 결과 (`<iidx16_extract>\textage\final_premium.csv`, `final_empress.csv`):
- PREMIUM BEST 99곡: 전부 키가 있고, 키가 속한 블록(17)의 CS 레벨이 디스크 레벨과 일치했다(97곡은 자동, GENTLE STRESS→`gentle`, THE EARTH LIGHT→`theearth`는 레벨 정확 일치로 수동 확정). `gentle`은 `titletbl.js`에 제목 행이 없어서 아티스트·장르가 비어 있다.
- EMPRESS 100곡: 96곡 레벨 일치. 나머지는 다음과 같다.
  - CYBER FORCE -DJ YOSHITAKA REMIX- → `cyber_4c`: 제목은 맞고, DP HYPER 한 값만 다르다(디스크 1, CS 4).
  - NO CRIME J → `no_crime`: 사용자 확인에 따라 NO CRIME의 일본어판으로 보고 연결했다. SP BEGINNER 값이 다르다(디스크 1, CS 2).
  - NEOGENESIS(`neogenes`), OUR SONG(`our_song`): 제목은 맞지만 레벨이 다르다. 확인이 필요하다.

### 0.3 난이도 후보 + 제목 대조 (제안 매핑)

미매칭 곡마다 같은 레벨 10개를 가진 Textage 키를 찾고, 제목을 눈으로 대조했다. 결과는 `<iidx16_extract>\textage\proposed_disc2_premium.csv`, `proposed_disc1_empress.csv`에 있다.

| 디스크 | 자동 확정 | 제안 (레벨 + 제목 대조) | 남음 |
|---|---|---|---|
| PREMIUM BEST | 79 | 18 | 2 (GENTLE STRESS, THE EARTH LIGHT: 같은 레벨 후보 없음) |
| EMPRESS | 70 | 26 (24는 레벨 일치, 2는 제목만 일치) | 2 (CYBER FORCE -DJ YOSHITAKA REMIX-: 후보 없음, NO CRIME J: NO CRIME과 레벨이 달라 별도 버전으로 보임) |

확인 필요한 제안:
- EMPRESS NEOGENESIS(`neogenes`), OUR SONG(`our_song`): 제목은 맞지만 디스크 레벨과 CS 레벨이 다르다. 레벨 바이트 또는 CS 행을 다시 봐야 한다.
- PREMIUM SETSUGEKKA: `_stgekka`와 `_stge_cs`가 둘 다 "雪月花"다. 어느 쪽이 PS2 판인지 확인이 필요하다.
- EMPRESS TECHNO BANCHO → `_rage16`(天空脳番長危機十六連打): 제목이 달라 보이지만 레벨이 일치한다. 디스크 카드의 한자 제목과도 맞는다.

남은 일: 제안을 확인하고, 미확정 4곡은 수동으로 `textage_key`를 채운다.

대안 후보 (확인 필요):
- Textage의 CS 표(있다면 직접 파싱)
- RemyWiki EMPRESS + PREMIUM BEST 페이지(https://remywiki.com/CS_EMPRESS) 표 파싱
- 사람이 직접 적은 CSV (디스크 곡 ASCII 이름 기준)

## 1. DB 개요 (README 기준, 다운로드 전 확인한 내용)

- 저장소: https://github.com/tts1374/iidx_all_songs_master
- 파일 이름: `song_master_<UTC 날짜>.sqlite` (예: `song_master_2026-02-22.sqlite`)
- 배포: GitHub Releases (날짜 태그). `latest.json`에 현재 파일 이름, SHA-256, 크기가 있다.
- 원천: Textage의 `titletbl.js`, `datatbl.js`, `actbl.js`를 빌드 스크립트가 읽어서 만든다.
- 라이선스: Apache-2.0 (README 표기)

### 1.1 테이블

| 테이블 | 내용 | 이번 작업에서 쓰는 필드 |
|---|---|---|
| `music` | 곡 1행 | `music_id`, `textage_id`, `title`, `title_qualifier`, `title_search_key`, `artist`, `genre`, `version` |
| `chart` | 플레이 스타일·난이도별 1행 | `play_style`(SP/DP), `difficulty`, `level`, `notes` |
| `music_title_alias` | 제목 별칭 | 제목이 안 맞을 때 보조 |
| `inf_pack`, `meta` | INFINITAS 팩, 스키마 정보 | 이번 작업에서는 사용하지 않음 |

### 1.2 확인하지 못한 것

- EMPRESS와 PREMIUM BEST 곡이 DB에 모두 있는지: README에 명시가 없다. 다운로드해서 `version` 값을 봐야 한다.
- 일본어 제목이 `title`에 들어 있는지, `title_search_key`에는 무엇이 들어 있는지: 실제 DB에서 확인 필요.

## 2. 디스크 쪽 입력 (이미 확보)

| 항목 | 위치 | 비고 |
|---|---|---|
| 곡 ID | 곡 레코드 순번 (디스크 2: `0x17e0f0`, 디스크 1: `0x178bf0`, 각 `0x134`바이트) | 폴더 `[xxxx]`와 일치 |
| ASCII 제목 | 레코드 첫 0x20바이트 | 예: `ZERO-ONE`, `TAMAYURA`, `D.A.N.C.E.!` |
| 난이도 레벨 | 레코드 `0x4a`부터 10바이트 | SP NORMAL, HYPER, ANOTHER, BEGINNER, BLACK, DP NORMAL, HYPER, ANOTHER, BEGINNER, BLACK (0 = 없음) |
| 아티스트 | 아틀라스 (`0xB4` 슬롯, 컨테이너 0x523) | 이미지. DB의 `artist`로 검증 |
| 장르 | 아틀라스 (`0xAC` 슬롯, 컨테이너 0x522) | 이미지. DB의 `genre`로 검증 |
| 제목(그림) | 아틀라스 (`0xB0`, `0xB8` 슬롯) | 이미지. DB 제목과 대조 가능 |

디스크 2 기준 곡 수는 99곡이다. 디스크 1(EMPRESS)은 100개 테이블 항목 중 0~99번이 곡이고, 100~103번은 제외한다.

## 3. 매핑 전략

### 3.1 1단계: 후보 집합 좁히기

1. DB에서 `version`이 EMPRESS, PREMIUM BEST에 해당하는 곡을 고른다. 버전 번호나 이름은 다운로드 후 실제 값으로 정한다.
2. 곡별 `chart` 행을 묶어서 `(SP/DP, difficulty) → level` 집합을 만든다.

### 3.2 2단계: 자동 매칭 (우선순위 순)

| 순서 | 조건 | 판정 |
|---|---|---|
| A | ASCII 제목 정규화 후 `title_search_key` 또는 `title`과 일치 + 레벨 집합 일치 | 확정 |
| B | 제목 일치, 레벨 집합 불일치 | 후보 표시, 검토 |
| C | 제목 불일치, 레벨 집합과 노트 수 일치 | 후보 표시, 검토 |
| D | 둘 다 불일치 | 수동 매핑 (별칭 CSV) |

- 정규화: 대문자화, 공백·하이픈·구두점 제거. 예: `D.A.N.C.E.!` ↔ `DANCE`처럼 차이가 큰 경우는 D로 간다.
- 레벨 비교: 디스크 레벨 10바이트에서 0을 제외하고 DB `chart.level`과 비교한다. DP 레벨은 디스크에서 DP NORMAL/ANOTHER 값을 DP BEGINNER/BLACK에도 복사하는 문제가 있다(`set_playlevel.py` 주석 참고). 그래서 DP 쪽은 가중치를 낮춘다.

### 3.3 3단계: 교차 검증 (이미지와 DB)

- 아티스트: 아틀라스 `0xB4` 슬롯 스프라이트 이미지를 잘라서 사람이 확인하거나, OCR로 읽은 텍스트를 DB `artist`와 비교한다.
- 장르: 같은 방식으로 `0xAC` 슬롯과 DB `genre`를 비교한다.
- 기준 곡: ZERO-ONE(디스크 65번) — 카드에서 본 값 `Mr.T`(아티스트), `ELECTROSHOCK`(장르)이 DB와 같아야 한다. ANISAKIS(디스크 0번)도 같은 방식으로 본다.

### 3.4 4단계: 결과물

곡마다 JSON 한 개(예: `song_meta/<디스크>/<곡번호>.json`):

```json
{
  "disc": "disc2",
  "song_id": 65,
  "ascii_title": "ZERO-ONE",
  "db_music_id": "...",
  "textage_id": "...",
  "title": "...",
  "artist": "...",
  "genre": "...",
  "version": "...",
  "levels": {"sp_normal": 5, "...": 0},
  "match": "A | B | C | D",
  "notes": "..."
}
```

- `match`: 어느 단계에서 확정했는지 기록한다. 검토가 필요한 곡을 쉽게 찾을 수 있다.
- 디스크 텍스처 경로(제목·아티스트·장르 사각형)도 함께 넣으면 플레이어가 이미지 표시와 텍스트 표시를 고를 수 있다.

## 4. 실행 순서

1. 최신 release에서 `song_master_*.sqlite`와 `latest.json`을 받는다. SHA-256을 `latest.json`과 대조한다.
2. SQL로 `version` 값 목록과 EMPRESS, PREMIUM BEST에 해당하는 곡 수를 확인한다.
3. 디스크 곡 목록(ASCII 제목, 레벨)을 CSV로 뽑는다. 곡 레코드에서 바로 뽑을 수 있다.
4. 1~2단계 매칭 스크립트를 돌리고, `match`별 개수를 센다.
5. D로 떨어진 곡은 수동 별칭 CSV로 처리한다.
6. 기준 곡(ZERO-ONE, ANISAKIS)으로 3단계 교차 검증을 한다.
7. 결과 JSON을 생성한다.

## 5. 리스크와 주의점

- **버전 차이**: DB의 `title`, `artist`가 PS2 버전과 다를 수 있다(예: 아케이드 표기, 후에 바뀐 표기).
- **레벨 불일치**: PS2 레벨은 아케이드와 다를 수 있다. 레벨이 다르면 A가 안 되고 B로 떨어진다.
- **DP 레벨 복사**: 위에서 말한 대로 DP BEGINNER/BLACK 값은 신뢰하지 않는다.
- **라이선스**: Apache-2.0이지만, 결과 JSON에 DB 원본을 그대로 넣어 배포하는 것은 피한다. 필요한 필드만 개인 용도로 쓴다.
- **재배포 금지**: 디스크에서 추출한 이미지와 음원은 배포하지 않는다. 메타데이터도 디스크 데이터와 섞어 공개하지 않는다.

## 6. 미정 사항 (다운로드 후 결정)

- EMPRESS와 PREMIUM BEST의 DB `version` 값
- 제목 정규화 규칙(특수문자 처리)과 별칭 CSV 형식
- 결과 JSON을 디스크별로 나눌지, 한 파일로 합칠지
