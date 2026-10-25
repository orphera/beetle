# 게임 디스크 곡 세부 정보 표 (`disc_meta`)

PS2 디스크에서 바로 플레이하는 곡(`docs/plans/2026-10-23-disc-direct-playback.md`)은 디스크에 영문 대문자 제목(`HIMIKO`)과 레벨만 들어 있다. 실제 제목·부제·아티스트·장르는 사람이 확인한 표로 채운다. 로딩 카드는 디스크 레코드에 적혀 있어 표가 필요 없다. 이 문서는 그 표의 형식, 초안을 만드는 방법, 사람이 채우는 방법을 정리한다.

## 1. 위치와 저장소 정책

- 위치: `library.dat`와 같은 폴더(게임의 작업 폴더)의 `disc_meta/<에디션>.tsv`

  | 디스크 | 파일 |
  |---|---|
  | beatmania IIDX 16 EMPRESS (Disc 1, `SLPM_552.21`) | `disc_meta/iidx16-empress.tsv` |
  | beatmania IIDX 16 PREMIUM BEST (Disc 2, `SLPM_552.22`) | `disc_meta/iidx16-premium-best.tsv` |
  | beatmania IIDX 14 GOLD (`SLPM_669.95`) | `disc_meta/iidx14-gold.tsv` |

- **표는 저장소에 넣지 않는다**(`.gitignore`의 `/disc_meta/`). 표의 제목·아티스트·장르는 Textage(textage.cc)에서 가져온 값이다. Textage의 Read Me는 개인적인 범위의 이용은 자유, 블로그 등에서는 textage.cc 주소를 밝혀 달라는 부탁만 하고, 데이터를 묶어 재배포하는 경우는 말하지 않는다. beetle 저장소는 공개 저장소이므로 표는 각자 만들어 쓴다. 저장소에는 표를 만드는 스크립트와 이 문서만 둔다.
- 표가 없으면 디스크의 영문 제목만 보인다. 플레이에는 지장이 없다.

## 2. 형식

UTF-8, 탭으로 구분한 텍스트 파일이다. 읽는 코드는 `crates/cs-disc/src/meta.rs`.

- `#`로 시작하는 줄과 빈 줄은 주석이다.
- 주석이 아닌 첫 줄이 열 이름이다. `id`만 꼭 있어야 하고, 나머지는 없어도 되며 순서도 자유다.
- 빈 칸은 "모름"이다. 디스크 값(영문 제목)이 그대로 쓰이거나, 그 정보가 비어 있게 된다.

| 열 | 내용 | 게임이 쓰는가 |
|---|---|---|
| `id` | 곡 번호: 디스크 곡 테이블의 레코드 순번(0부터) | 예 (필수) |
| `disc_title` | 디스크의 영문 제목. 사람이 보라고 두는 열 | 아니오 |
| `title` | 제목 (`卑弥呼`) | 예 |
| `subtitle` | 부제 (`-somatic mutation type "Forza"-`) | 예 |
| `artist` | 아티스트 | 예 |
| `genre` | 장르 | 예 |
| `note` | 메모 | 아니오 |

예 (탭은 ` | `로 표시):

```
# beatmania IIDX 16 EMPRESS song details
id | disc_title | title | subtitle | artist | genre | note
47 | HIMIKO | 卑弥呼 |  | 朱雀 VS 玄武 | ESOTERIC SPEEDCORE | textage _himiko
```

### 2.1 바꿔도 기록은 그대로다

채보의 식별값(`ChartId`)은 표 없이 디스크만으로 만든 BMS에서 정한다. 표를 고치거나 지워도 점수·리플레이는 같은 채보에 이어진다. 표 파일이 곡 캐시(`songs.cache`)보다 새로우면 게임이 다음 실행 때 곡 목록을 다시 읽으므로, 고친 뒤 게임을 다시 켜면 된다.

### 2.2 글자

제목과 아티스트는 게임의 내장 폰트(라틴, 상용 한자 일본어, 한글)로 그린다. 상용 한자 밖의 한자(예: 아티스트 이름의 드문 한자)는 빈칸이나 대체 글자로 보일 수 있다.

## 3. 초안 만들기: Textage 매핑에서 (스크립트)

iidx16-bms 작업(별도 폴더)에서 만든 매핑이 있으면 스크립트로 두 표를 한 번에 만든다.

필요한 파일 (iidx16-bms 작업 폴더의 산출물):

| 파일 | 내용 |
|---|---|
| `textage/song_map_final.csv` | 디스크 곡 번호 → Textage 키, 제목, 아티스트, 장르. 제목+레벨로 자동 매칭되지 않은 곡도 사람이 맞춰 둔 확정본 |
| `textage/titletbl.js` | Textage 제목 표(UTF-8 변환본). 부제를 가져온다 |

게임 작업 폴더(`library.dat`가 있는 곳)에서 실행한다:

```bash
python scripts/disc_meta_from_textage.py --map <...>/textage/song_map_final.csv --titletbl <...>/textage/titletbl.js
```

- 결과: `disc_meta/iidx16-empress.tsv`, `disc_meta/iidx16-premium-best.tsv` (기존 파일을 덮어쓴다)
- `note` 열에는 Textage 키와, 자동 매칭이 아니었던 곡의 매칭 방식(`proposed(level+title)`, `manual…`)이 참고로 들어간다.
- 스크립트는 매번 표를 새로 만든다. 고칠 것이 있으면 **`song_map_final.csv`를 고치고 다시 실행**하는 것이 원칙이다. 표를 직접 고쳤다면 그 뒤로는 스크립트를 다시 돌리지 않는다.
- 스크립트는 제목을 대문자·영숫자만 남겨 비교한다(`DANCIN' INTO THE NIGHT` = `DANCIN INTO THE NIGHT`).

## 4. 사람이 채우기

매핑 파일이 없거나, 스크립트 결과에서 비거나 애매한 칸이 있으면 사람이 채운다.

### 4.1 빈 양식 만들기

디스크(ISO 또는 드라이브)에서 곡 번호와 영문 제목만 채운 양식을 만든다.

```bash
cargo run -p cs-disc --release --example disc_meta_template -- "D:\ISO\<디스크>.iso" disc_meta/iidx16-empress.tsv
```

드라이브는 `E:`처럼 준다. 출력 파일을 빼면 화면에 찍는다.

### 4.2 제목·부제·아티스트·장르

- 참고처: Textage(https://textage.cc/), RemyWiki의 CS EMPRESS / PREMIUM BEST 문서, 게임 화면의 로딩 카드(`card_dump`로 뽑은 그림에 장르·제목·아티스트가 적혀 있다).
- 같은 제목이 여러 개면(리믹스, CS 전용판) 디스크 레벨(`disc_meta_template`의 `disc_title`과 게임의 레벨 표시)과 CS 레벨이 맞는 쪽을 고른다.
- 부제는 따로 표시되므로 제목 칸에 함께 넣지 않는다(`THE SHINING POLARIS` / `(kors k mix)`).
- 확신이 없는 칸은 비워 둔다. 빈 칸은 디스크 값이 쓰이므로 틀린 값보다 낫다. 메모는 `note`에 적는다.

## 5. 관련 코드

| 위치 | 역할 |
|---|---|
| `crates/cs-disc/src/meta.rs` | 표 읽기 |
| `crates/cs-disc/src/iidx.rs` | 곡 테이블, 로딩 카드 번호(`Song::stage`, 레코드 0x94), 카드 그림(`Disc::card_image`, `Disc::stage_image`) |
| `crates/cs-disc/src/ps2tex.rs` | 카드 그림 디코딩(LZSS, PSMT8), 4:3 배치 |
| `crates/beetle-app/src/disc_meta.rs` | 게임이 표를 찾는 곳 |
| `crates/beetle-app/src/scanner.rs`, `loader/disc.rs` | 곡 목록과 플레이 화면에 표 반영, 로딩 카드 |
| `scripts/disc_meta_from_textage.py` | Textage 매핑에서 초안 만들기 |
| `crates/cs-disc/examples/disc_meta_template.rs`, `card_dump.rs` | 빈 양식, 카드 그림 뽑기 |
