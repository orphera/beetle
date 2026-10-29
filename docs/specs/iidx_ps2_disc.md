# PS2 beatmania IIDX 디스크 형식

`crates/cs-disc`가 읽는 PS2 IIDX 디스크(3rd style, 4th style, 5th style, 6th style, 7th style, 8th style, 9th style, 10th style, 11 IIDX RED, 12 HAPPY SKY, 13 DistorteD, 14 GOLD, 15 DJ TROOPERS, 16 EMPRESS, 16 PREMIUM BEST)의 데이터 형식과, 직접 디스크를 집계해서 알아낸 것을 모은다. 곡 세부 정보 표(제목·아티스트)는 [disc_meta.md](disc_meta.md), 재생 계획은 [2026-10-23-disc-direct-playback.md](../plans/2026-10-23-disc-direct-playback.md).

출처: iidx-ps2tools(`plugins/iidx*cs.py`, `parse_wvb.py`), SaxxonPike의 rhythm-game-formats(`iidx/1.md`, 아케이드 `.1`). 그 밖의 내용은 네 장의 디스크를 직접 집계한 결과이며, 집계 범위를 함께 적는다.

## 1. 에디션과 표 위치

부트 실행 파일(`SYSTEM.CNF`의 `BOOT2`)의 이름으로 에디션을 가린다(`titles.rs`, `iidx::Edition`). 곡 표, 파일 표, `DATA1.DAT` 표는 모두 실행 파일 안에 있고, 위치는 에디션마다 다르다. 다른 점은 `crates/cs-disc/src/iidx.rs`의 `Layout` 표 하나에 모여 있다.

| 에디션 | 실행 파일 | DAT 파일 | 레코드 크기 | 차트 키 |
|---|---|---|---|---|
| 3rd style | `SLPM_650.06` | `DX2_3/BM2DX3.BIN` + 실행 파일(채보) | 0x7c | 없음 (암호화 안 함) |
| 4th style | `SLPM_650.26` | `DX2_4/BM2DX4.BIN` + 실행 파일(채보) | 0x90 | 없음 (암호화 안 함) |
| 5th style | `SLPM_650.49` | `DX2_5/BM2DX5.BIN` + 실행 파일(채보) | 0xa4 | 없음 (암호화 안 함) |
| 6th style | `SLPM_651.56` | `DX2_6/BM2DX6A/B.BIN` + 실행 파일(채보) | 0xa0 | 없음 (암호화 안 함) |
| 7th style | `SLPM_655.93` | `DX2_7/BM2DX7B/C/A.BIN` | 0xa0 | 없음 (암호화 안 함) |
| 8th style | `SLPM_657.68` | `DX2_8/BM2DX8B/C/A.BIN` | 0x9c | 없음 (암호화 안 함) |
| 9th style | `SLPM_659.46` | `DATA2.DAT` 하나 | 0x16c | 없음 (암호화 안 함) |
| 10th style | `SLPM_661.80` | `DATA2.DAT` 하나 | 0x16c | 없음 (암호화 안 함) |
| 11 IIDX RED | `SLPM_664.26` | `DATA2.DAT` 하나 | 0x140 | 없음 (암호화 안 함) |
| 12 HAPPY SKY | `SLPM_666.21` | `BM2DX12A/B/C.DAT` | 0x144 | 없음 (암호화 안 함) |
| 13 DistorteD | `SLPM_668.28` | `BM2DX13A/B/C.DAT` | 0x118 | 없음 (암호화 안 함) |
| 14 GOLD | `SLPM_669.95` | `BM2DX14A/B/C.DAT` | 0x11c | `sEk2I1Xq7bz09Mtv` |
| 15 DJ TROOPERS | `SLPM_551.17` | `BM2DX15A/B/C.DAT` | 0x134 | `eqkBipx2wgExUAIG` |
| 16 EMPRESS | `SLPM_552.21` | `BM2DX16A/B/C.DAT` | 0x134 | `uYlFgVwOpaQmXdIR` |
| 16 PREMIUM BEST | `SLPM_552.22` | `BM2DX16A/B/C.DAT` | 0x134 | `uYlFgVwOpaQmXdIR` |

- 파일 표: 12바이트 레코드(DAT 안 섹터, 미사용, 바이트 수). 10–13은 8바이트 레코드(섹터, 섹터 수). DAT 파일이 여럿이면 그 순서로 번호가 이어진다.
- `DATA1.DAT` 표: 16바이트 레코드(id, 섹터, 섹터 수, 0).
- 차트 파일은 Blowfish(위 키)로 암호화하고 LZ로 압축되어 있다. 10–13은 LZ 압축만 한다.

### 1.1 8th style

- `DATA1.DAT`가 없다. 파일 표는 16바이트(섹터, 미사용, 바이트, 해시)이고 B(116개), C, A 순서로 이어진다.
- 파일 번호(`FileNumbers::Split`): 채보는 `0xf0000000 | n`으로 C의 n번(전체 116+n). 키음·BGM·영상은 `u16`이고 116 미만이면 B, 116 이상이면 C의 그 번호.
- 곡 레코드 0x9c: 0x00 ASCII 제목 주소, 0x04 표시 제목 주소(주소 − 0xfff80 = 파일 오프셋), 0x08 레벨 6바이트(SP H/A, DP H/A, SP N, DP N, SP B, DP B), 0x14 영상 `u16` 둘(MPEG-2, 거의 같은 두 판, 첫째를 씀), 0x54 채보 8개, 0x74 사운드 16개.
- 87~89번은 제목이 공백이다(그림으로만 이름이 있음). 레벨로 太陽(TAIYO)·Don't be afraid myself·spiral galaxy를 가려 `Layout::titles`에 둔다.
- 키음 뱅크는 옛 형식(`wvb.rs` 모듈 설명: 첫 워드가 파일 길이, 0x20~0x4000에 32바이트 항목, 모노, 암호 없음). BGM은 빅엔디언 헤더(크기, ?, 레이트, 채널) 뒤 0x800부터, 암호 없음.
- 채보는 시작에 BGM 1과 함께 소리 2를 낸다. 어느 뱅크에도 없고 BGM이 이미 스테레오라 아무것도 내지 않는다(`Layout::bgm_pair`, 추정).
- 자기 키음이 없는 채보는 같은 난이도의 다른 스타일 것을 먼저 쓴다(ABSTRACT DP NORMAL → SP NORMAL). 9th~16에서도 결과가 같다.
- 채보 7개가 뱅크 어디에도 없는 소리를 부른다(GRAVITY SP NORMAL의 3 등). 디스크에 없으므로 무음으로 둔다.
- 첫 마디선이 여러 마디를 덮고 그 안에서 BPM이 바뀌는 채보(DUAL CONTROL)가 있다. 마디 길이는 템포 구간마다 따로 재어 더한다(`ply_bms::bars`).

### 1.2 7th style

- 8th와 같은 틀이다(`SEVENTH_STYLE`, 8th 레이아웃에서 바뀐 것만 둠). B.BIN이 138개라 나눔 기준이 138이다.
- 곡 레코드 0xa0: 0x10 레벨 6바이트(8th와 같은 순서), 0x1a 영상 `u16`(MPEG-2), 0x58 채보 8개, 0x78 사운드 16개. iidx-ps2tools의 7th 읽기 순서로 셈한 위치(0x4c·0x6c)는 맞지 않아 레코드를 직접 보고 정했다.
- 채보 22개가 뱅크에 없는 소리를 부른다(무음).
- MAX 300의 끝부분은 한 마디 안에서 BPM이 277→12→300으로 여러 번 바뀐다. 마디 안 변화 위치를 BMS로 쓸 때 새 BPM으로 다시 스냅하면 더 굵은 격자로 옮겨져 뒤 노트가 223ms 일찍 나왔다. 맞춤 단계의 박자를 그대로 쓴다(`ply_bms::exact_pos`). 다른 디스크도 오차가 같거나 줄었다.

### 1.3 6th style

- 채보가 실행 파일 안에 있다. 채보 번호가 실행 파일 주소이고(주소 − 0xff000 = 파일 오프셋) LZ로 압축되어 있다. 실행 파일을 DAT 하나로 더하고, 채보마다 다음 채보 시작까지를 파일로 둔다(`exe_charts`). 형식은 Ps2Short(헤더 없는 4바이트 시간이 아니라 짧은 헤더).
- 제목 주소의 기준은 0xff000이다.
- 레코드 0x16의 `u16`이 곡의 사운드·영상이 든 BIN이다(0·1 A, 2 B). B의 번호는 A 345개 뒤로 이어진다(`FileNumbers::Archives`).
- 레코드 0x10 레벨(7th와 같은 순서), 0x28 영상, 0x58 채보, 0x78 사운드.
- 곡 표 215개 중 앞의 69개만 곡이다(RemyWiki 69곡과 같음). 그 뒤는 `D` 15개, `END`, `d001`~`d130`으로 모두 레벨 1인 자리 채움이라 읽지 않는다.
- 단위인정이 없다(7th부터). 채보 71개가 뱅크에 없는 소리를 부른다(무음).

### 1.4 5th style

- 6th처럼 채보가 실행 파일 안(LZ, 주소 − 0xff000)이고, BIN은 하나라 사운드·영상 번호가 그대로 파일 번호다(`FileNumbers::Executable`).
- 곡 레코드 0xa4: 0x08 레벨(6th와 같은 순서), 0x12 영상 `u16`(그다음 번호가 둘째 영상), 0x5c 채보 8개, 0x7c 사운드 16개.
- 곡 표 앞 64개가 곡이다(RemyWiki 64곡). 그 뒤 `(ENDING)`, `SUV. CONG.1~3`(서바이벌 모드), `d01`~`d73`(드릴 모드로 보임)은 읽지 않는다.
- 채보 81개가 뱅크에 없는 소리를 부른다(무음).

### 1.5 4th style

- 5th와 같은 틀이다. 파일 표는 12바이트(섹터, 바이트, 해시, `FileRecords::ShortHashed`), 채보 형식은 Ps2NoHeader.
- 곡 레코드 0x90: 0x09 레벨(HYPER·ANOTHER 공용, SP·DP 공용), 0x0a NORMAL 레벨, 0x0e 영상, 0x58 채보 6개(BEGINNER 없음, SP H/DP H/SP A/DP A/SP N/DP N), 0x70 사운드 12개.
- 곡 표 앞 53개가 곡이다(RemyWiki 53곡). 그 뒤 `(ENDING)`과 `SUV. CONG.1~3`.
- 채보 32개가 뱅크에 없는 소리를 부른다(무음).

### 1.6 3rd style

- 4th와 같은 틀이다. 곡 레코드 0x7c: 0x0a 영상, 0x0c 레벨(HYPER·ANOTHER, NORMAL), 0x54 채보 6개, 0x6c 사운드 4개(SP HYPER와 DP HYPER의 키음·BGM뿐, `Record::sound_count`). 다른 채보는 이 둘을 빌려 쓴다.
- NORMAL 채보 4개(PANORAMA, OVERDOSER, E-MOTION)는 빌려 쓰는 SP HYPER 뱅크가 아니라 DP HYPER 뱅크의 소리를 몇 개 낸다. 플레이어는 채보의 뱅크에 없는 소리를 곡의 다른 뱅크에서 찾는다(`Song::key_banks`).
- 곡 표 앞 81개가 곡이다(RemyWiki 81곡). 그 뒤 `(ENDING)`.
- 4th의 사운드는 12개다(16개로 읽으면 레코드 끝의 다른 값을 뱅크 번호로 읽는다). 엉뚱한 파일을 뱅크로 읽어도 할당이 터지지 않게, 파일 개수가 파일 크기를 넘는 뱅크는 거부한다.

## 2. 곡 레코드

| 필드 | 10th | 11 RED | 12 HAPPY SKY | 13 DistorteD | 14 GOLD | 15·16 |
|---|---|---|---|---|---|---|
| 제목 (ASCII, 대문자 약칭) | 0x00, 0x40바이트 | 0x00 | 0x00 | 0x00 | 0x00 | 0x00 |
| 레벨 바이트 | 0x41부터 | 0x41부터 | 0x41부터 | 0x42부터 | 0x42부터 | 0x4a부터 |
| 배경 영상 파일 번호 | 0x58 | 0x5c | 0x54 | 0x54 | 0x54 | 0x58 |
| 스테이지 그림 | 0xd4 (0xac는 BGA 오버레이) | 0xd8 (0xb0은 오버레이) | 0xcc (0xa4는 오버레이) | 0x88 (0x60은 BGA 오버레이) | 0x84 (DATA1 파일 번호) | 0x94 |
| 차트 파일 번호 10개 (10–13은 8개, BLACK 없음) | 0x12c | 0x100 | 0xec | 0xb8 | 0xb4 | 0xc0 |
| 사운드 파일 번호 16개 | 0x14c | 0x120 | 0x10c | 0xd8 | 0xdc | 0xe8 |

- 차트 순서: SP H, DP H, SP A, DP A, SP N, DP N, SP B, DP B, SP BLACK, DP BLACK.
- 레벨: 15·16은 SP N/H/A/B/BLACK, DP N/H/A/B/BLACK 순의 10바이트. DJ TROOPERS에서 DAZZLIN' DARLIN' DP BEGINNER가 8번 바이트에 1, RISTACCIA DP BLACK이 9번 바이트에 11(DP ANOTHER는 10)로 확인했다.
- 10th 레벨: 비기너 1바이트(0x41), 미사용 8바이트, SP H/A, SP N, DP H/A, DP N. HYPER와 ANOTHER는 레벨 하나를 함께 쓴다(PANDORA 7, 5, 8, 6). RemyWiki의 10th style CS 레벨과 노트 수(PANDORA, BABY LOVE, No.13)도 이와 같다.
- 더하기 레벨: 디스크는 정수만 적고, "+" 레벨은 다음 정수로 적는다. RED의 9는 RemyWiki의 "8+"다(BLOCKS SP·DP ANOTHER). 10th는 1–8만 쓴다. 9th CS의 "7+"는 10th CS에서 8이 되었다(No.13). 플레이어는 디스크 값 그대로 보여 준다.
- RED 레벨: 비기너 1바이트, 미사용 10바이트, SP H/N/A, DP H/N/A. HAPPY SKY: 비기너, 미사용 8바이트, SP N/H/A, DP N/H/A.
- GOLD·DistorteD 레벨: 비기너 1바이트(SP·DP 공용), 미사용 9바이트, SP N/H/A, DP N/H/A. BLACK은 ANOTHER의 레벨을 쓴다.
- 사운드 파일 번호는 차트 쌍마다 (키음 뱅크, BGM). 차트에 자기 것이 없으면 DP HYPER(더블) 또는 SP HYPER의 것을 쓴다.
- 레코드에는 곡별 음량 값이 없다. EMPRESS에서 곡의 실제 음량과 상관있어 보이는 0x54 바이트를 검토했지만 곡 순서를 따르는 값이었다(5.2절).

## 3. 차트 (`.ply`)

헤더 `u32 첫 이벤트 오프셋, u32 multiplier` 뒤에 8바이트 이벤트 `u32 time, u8 c1, u8 c2, u16 param`이 이어지고, time 0x7fff에서 끝난다. `c1`의 하위 니블이 명령, 상위 니블이 슬롯(레인)이다.

### 3.1 시간 단위

- 시간 단위는 `multiplier / 1000` ms. 네 장의 모든 차트가 0x412b(16.683ms, 59.94Hz 한 프레임)를 쓴다.
- 아케이드 GOLD의 `.1`은 60.046Hz 틱을 쓰지만, PS2 GOLD는 아니다. 일정 템포 89곡의 마디선 간격과 BPM으로 재면 중앙값이 59.945 프레임/초로 나온다.
- 11·12의 몇 곡은 박자 명령 없이(처음의 4/4 하나뿐) 마디선 간격만으로 박자를 바꾼다. GOLDEN HORN(RED)은 7/4 마디가 이어지고, 100% minimoo-G(HAPPY SKY)는 마디마다 3–9박이다. 그래서 마디선 간격이 저장된 템포에서 박자와 크게 다르면(0.2박 + 3% 넘게) 그 간격을 4분음표 단위로 반올림한 길이를 마디 길이로 쓴다.
- 저장된 BPM은 정수로 반올림되어 있어, 시각은 프레임이 정한다. 마디 안에서 템포가 바뀌는 곡(THE LEAST 100sec 20–21.5초)도 있다. 마디와 템포 맞춤은 `ply_bms::Layout` 하나가 하고, BMS 변환과 게임의 네이티브 로더가 함께 쓴다.

### 3.2 명령

| 명령 | 내용 |
|---|---|
| 0, 1 | 노트(P1, P2). 슬롯 = 레인(0–6 건반, 7 스크래치), param = 롱노트 길이 |
| 2, 3 | 키음 지정(P1, P2). param = 사운드 번호. 소리는 내지 않는다 |
| 4 | 템포. 슬롯이 상위 바이트, `c2`가 하위 바이트 |
| 5 | 박자 |
| 6 | 곡 끝 |
| 7 | 배경음. param = 사운드 번호, 슬롯 = 팬(3.4절). 사운드 1은 BGM 스트림 |
| 8 | 판정 범위(3.5절) |
| 0xb | GOLD에만 107개, 값이 모두 0. 뜻 모름 |
| 0xc | 마디선 |

### 3.3 시간 0의 노트 = 노트 수

시간 0에 있는 레인 0 노트는 노트가 아니라 그 쪽의 노트 수다. `c2`에 250씩 나누어 적는다.

- 2HO SP HYPER: 250, 250, 112 → 612노트
- 2HO DP HYPER: P1 250, 54 → 304 / P2 250, 81 → 331
- 네 장의 모든 차트(2,851개)에서 일치했다. 아케이드 `.1`의 노트 수 이벤트(0x10)와 같은 정보다.
- 그 밖의 노트와 키음 지정, 배경음의 `c2`는 0이다(예외: 3.4절의 SATELLITE).
- 그래서 플레이와 변환에서는 시간 0의 노트를 버린다.

### 3.4 배경음의 팬

슬롯 니블이 팬이다. 아케이드 `.1`의 07 이벤트 파라미터와 같은 값으로, 1 왼쪽, 8 가운데, 15 오른쪽이다.

| 디스크 | 8 | 1 | 15 | 0 |
|---|---|---|---|---|
| GOLD | 641,686 | 26 | 34 | – |
| DJ TROOPERS | 761,213 | 5 | 24 | 2,859 |
| EMPRESS | 742,236 | 7 | 8 | 39,565 |

- 슬롯 0: EMPRESS 십여 곡(FROM TIME TO TIME, BAD MANIACS, DOMINION 등)과 DJ TROOPERS SATELLITE가 곡 내내 쓴다. 전부 모노 사운드다. 뜻은 모르고, 아마 가운데다. SATELLITE의 슬롯 0 배경음에만 `c2` = 4가 있다.
- PREMIUM BEST: 팬 1인 스테레오 사운드 71개 옆에 팬 15인 모노 사운드 71개가 나온다. 한 사운드를 좌우로 나누어 내는 것으로 보인다.
- 플레이어는 팬을 쓰지 않는다(모두 가운데). 위 PREMIUM BEST 구간에서는 오른쪽 소리가 겹쳐 나올 수 있다.

### 3.5 판정 범위

시간 0의 명령 8 여섯 개(슬롯 0–5)가 프레임 수를 적는다. 짝으로 보면 BAD, GOOD, GREAT 경계로 보이지만, 의미와 부호(어느 쪽이 빠른지)는 실제 게임과 맞춰 보지 않았다. SaxxonPike 문서의 아케이드 정의는 "음수가 늦음, 양수가 빠름"이다.

| 값 | 쓰는 곳 (예) |
|---|---|
| −16, −6, −1, 3, 8, 18 | 기본값 (GOLD 597, DJ TROOPERS 634, EMPRESS 643 차트) |
| −16, −10, −2, 4, 12, 18 | DJ TROOPERS SATELLITE, EMPRESS JACK 등 (넓음) |
| −10, −6, −1, 3, 8, 12 | SPICA 등 (좁음) |
| −10, −6, −2, 4, 8, 12 | RINTOSHITE, GOLD STA 등 |
| −12, −8, −1, 3, 10, 14 | THE SHINING POLARIS, DR.LOVE |
| −10, −7, −1, 3, 9, 12 | GRADIUSIC CYBER DP HYPER만 |
| −16, −4, 0, 2, 6, 18 | GOLD 20N ANOTHER |

같은 곡 안에서도 난이도마다 다를 수 있다. 플레이어는 이 값을 쓰지 않고 자체 판정 규칙을 쓴다.

## 4. 사운드

### 4.1 키음 뱅크 (`.wvb`)

- `0x10..0x8000`: 키음 하나당 16바이트. `u16 channel, u16 kind, u32 길이(ms), u8 팬, u8 팬, u16 파일, u8 음량, 3바이트`.
- `0x8000`: 파일 수, 암호화 여부, 파일 레코드(오프셋, 크기, 샘플레이트 코드), 데이터. 데이터는 PS2 ADPCM이다.
- 암호화: 프레임마다 바이트 0을 0xff로 XOR하고 바이트 1·2·3에 키를 더한다. 14–16은 (0, 0xdc, 0), 11 RED는 (0xfe, 0xef, 0x37)이다. RED의 끝 프레임은 `00 07 77 77` 대신 `ff 05 66 ae`로 저장되며, 플레이어는 이것으로 키를 고른다. 10·12·13과 팝픈 12–14는 암호화하지 않는다. RED 키를 모를 때는 RED 키음이 전부 최대 음량의 잡음으로 디코딩됐다(vgmstream도 읽지 못한다).
- kind의 상위 바이트:
  - 2: 바이트 8 = 1, 바이트 9 = 127. 저장된 그대로 재생한다.
  - 1: 바이트 8과 9가 같은 값이고, 이것은 팬이다(1 왼쪽, 64 가운데, 127 오른쪽). 팝픈에서는 64를 중심으로 84/44, 94/34처럼 좌우 짝을 이루고, 10th에는 1(맨 왼쪽)도 181개 있다. 예전에는 이 값을 음량으로 보았는데, 그러면 왼쪽 끝 키음이 사실상 들리지 않았다(팝픈의 키음 누락).
- 바이트 12가 음량(0–127, 선형)이다. 마스터마다 다른 크기를 맞추는 값이라 곡 안에서도 34–127로 갈린다. GOLD THE LEAST 100sec의 잘린 음악 조각은 34와 60, 다른 키음은 86–127이다. 차트가 쓰는 키음 중 0은 디스크마다 수십 개뿐인 길이 1ms 남짓의 더미다. 팝픈은 대부분 108–127이다.
- 플레이어는 키음을 `음량 / 127`로 줄이고, kind 1 모노 키음은 팬에 따라 먼 쪽만 줄여(가운데는 양쪽 그대로) 스테레오로 만든다.
- channel: 0x0000 다음 0x0100 엔트리가 오면 스테레오 한 쌍(왼쪽, 오른쪽)이다. 오른쪽 엔트리도 따로 재생되는 사운드다. GOLD와 DJ TROOPERS에는 0x0100–0x0700 값이 더 있는데 뜻은 모른다.

### 4.2 BGM 스트림

0x800바이트 헤더(데이터 시작, 크기, 샘플레이트, 채널 수, 암호화, 인터리브 16, 0x28에 음량) 뒤에 채널마다 16바이트씩 번갈아 놓인 ADPCM 프레임.

곡 전체를 키음으로 재생하는 곡(DistorteD의 DUE TOMORROW·DRUNK MONKY, GOLD의 SH)은 BGM 자리에 1초가 안 되는 음량 1의 자리채움 스트림을 둔다. 차트는 이것도 다른 곡처럼 한 번 튼다.

음량은 키음과 같은 0–127 눈금이다. 큰 마스터일수록 값이 작다. RED에서 음량 × RMS는 곡마다 대략 일정하고(ANDROMEDA 37 × 10840, ROK DA WORLD 120 × 2537), 선형으로 보면 가장 잘 맞는다. 이것을 쓰지 않으면 큰 BGM이 키음을 덮고(10–12의 키음 누락), 리미터가 오래 눌려 소리가 뭉개진다(RED GO BERZERK, 음량 54).

## 5. 재생할 때의 음량

### 5.1 왜 맞춰야 하는가

디스크는 BGM과 키음을 각각 거의 최대 음량으로 마스터링해 두고, 여유분은 콘솔의 믹서에 맡긴다. 그대로 더하면 많은 곡이 최대를 넘는다. 오토플레이 믹스에서 최대를 넘는 시간의 비율은 다음과 같았다.

| 디스크 | 1%를 넘는 곡 | 가장 심한 곡 |
|---|---|---|
| EMPRESS | 34 / 102 | HOLIC 21%, MEI 14% |
| PREMIUM BEST | 51 / 99 | G2 19% |
| DJ TROOPERS | 32 / 103 | G2 19% |
| GOLD | – | RID 11% |

### 5.2 곡 전체 음량 정보는 없다

BGM과 키음 각각의 음량(4절)은 있지만, 그 둘을 더한 결과를 맞추는 값은 없다.

- 아케이드 `.1`: 음량 필드가 있는 이벤트가 없다. 소리 관련 값은 배경음의 팬뿐이다.
- PS2 `.ply`: 3절의 모든 바이트를 집계했지만 음량으로 볼 만한 값이 없다.
- 곡 레코드: EMPRESS의 0x54 바이트(대부분 16, 몇 곡이 3–12)가 후보였다. 하지만 이 값으로 곡을 맞추면 오히려 음량 차이가 커지고, 낮은 값은 곡 76–84번에 순서대로 붙어 있다. 음량이 아니다.

### 5.3 플레이어의 방법

- 디스크 곡: 곡을 불러올 때 오토플레이 믹스를 한 번 만들어(평균 약 0.1초), 10ms 구간 피크의 99번째 백분위가 리미터 문턱(0.9)에 오도록 줄인다. 그보다 조용한 곡은 마스터링 그대로 재생한다(`crates/beetle-app/src/loader/disc.rs`의 `song_gain`). GOLD와 EMPRESS의 중앙값은 약 0.6, 가장 큰 곡은 0.25–0.32다.
- 믹서: 모든 소리의 마지막 단에 지연 없는 리미터가 있다(`crates/beetle-audio/src/limiter.rs`). 0.9를 넘으려 하면 1ms 안에 줄이고 80ms에 걸쳐 되돌리며, 넘친 부분은 소프트 클립으로 둥글게 한다. 디스크 곡에서는 각 곡의 약 1% 구간에서만 작동한다.

## 6. 스테이지 그림

- 10–15: 레코드가 가리키는 DATA1 파일은 애니메이션 묶음이고, 그 1번 파일이 스테이지 그림(PSMT8 텍스처)이다.
  - 10·11·12도 13처럼 iidx-ps2tools가 읽는 오버레이 칸 0x28바이트 뒤가 스테이지 그림이며 모든 곡에 있다. 384×384 텍스처에 장르·제목·아티스트 카드 한 장이다. 그 뒤의 4바이트 칸들은 13의 그림 선택 칸과 모양은 같지만 값이 85까지 가므로 그림 선택이 아니다. 플레이어는 쓰지 않는다.
  - 예외로 RED의 A와 AA는 640×384 텍스처에 298픽셀 폭 그림 두 장(글자 모양이 조금 다르다)이 있다. 고를 칸이 없으므로 평소에는 왼쪽, ANOTHER에는 오른쪽을 쓴다.
  - 13은 0x88의 파일이 스테이지 그림이다. iidx-ps2tools가 읽는 0x60은 BGA 오버레이 묶음이며 27곡에만 있다.
  - 텍스처 하나에 298픽셀 폭의 그림 두 장이 나란히 들어 있을 수 있다. 어느 그림을 쓸지는 레코드의 그림 선택 칸에 적혀 있다. 칸은 4바이트 (그림 번호 u16, 파일 u16)이고, 그림 번호 0은 왼쪽, 1은 오른쪽이며, 파일이 0인 칸은 비어 있다.

    | 에디션 | 칸 위치 | 칸 수 | ANOTHER 칸 |
    |---|---|---|---|
    | 13 DistorteD | 0x8c | 6 | 3 (2는 CARDINAL GATE) |
    | 14 GOLD | 0x88 | 6 | 3 |
    | 15 DJ TROOPERS | 0x98 | 4 | 2 |

  - 0번 칸이 평소 카드다. ANOTHER 칸이 있는 곡은 ANOTHER 채보가 다른 믹스인 곡뿐이다. DistorteD는 OUTER LIMITS → ALTERNATIVE, Attack the music → 49 MUSIC MIX, super highway → SUPER SUBWAY MIX(RemyWiki), GOLD는 DENIM → ELECTRO MIX, DJ TROOPERS는 CRYMSON → SCARLET MIX, Beautiful Angel(아티스트 표기만 다름)이다. 플레이어는 0번 칸을 쓰고, ANOTHER이면서 ANOTHER 칸이 있으면 그 칸을 쓴다(`Song::stage_picks`).
  - 그 밖의 칸(GOLD·DistorteD의 1·4·5번)은 양쪽 그림이 같은 곡에만 있고, 쓰임은 모른다.
  - DistorteD의 CARDINAL GATE 5곡은 한 장이 그 모드에서 쓰는 카드다. 그 모드는 곡을 아티스트 이름으로 보여 주며(RemyWiki `CS_DistorteD`), 카드에 그 이름(朱雀, 玄武, 青龍, 白虎, 嘆きの樹)이 크게 쓰여 있다. 2번 칸이 이 카드를 가리킨다. CONTRACT·Ganymede는 이 카드가 오른쪽에 있고, 나머지 3곡은 왼쪽에 있어서 0번 칸이 1이다.
  - DJ TROOPERS의 텍스처는 대부분 폭이 넓어도 그림이 한 장이다. 그래서 어느 레코드든 그 파일의 1번 그림을 가리킬 때만 두 장으로 본다. GOLD의 5KEYS용 DENIM 항목은 칸이 0번뿐이지만, 같은 파일을 쓰는 본 항목이 1번 그림을 가리키므로 두 장으로 본다.
- 16: 로딩 카드. 레코드 0x94가 카드의 DATA1 파일 번호다(15의 스테이지 그림과 같은 자리). EMPRESS는 예전에 손으로 채운 곡 세부 정보 표의 `card` 열 100개(지금은 없앴다)와, PREMIUM BEST는 예전의 단어 쌍 추측 규칙과 모두 일치한다. 단 PREMIUM BEST의 MENDES 하나만 다르다. 예전 규칙은 레코드 끝 0x120의 `(0, 0x52d)`를 골라 BALLAD FOR YOU의 카드를 보여 주었다. 0x98부터 (그림 번호, 파일) 칸이 있지만 1번 그림을 쓰는 칸은 없다.

## 7. 단위인정 (CLASS 모드) 코스 표

모든 에디션(10th~16)의 실행 파일에 단위인정 코스 표가 있다. 10th~13과 14 이후는 형식이 다르다(`DanTable`). `crates/cs-disc/src/iidx.rs`의 `Layout::dan`이 그 위치이고, `read_dan_courses`가 읽는다.

| 에디션 | 실행 파일 안 위치 |
|---|---|
| 7th style | SP `0x1c5ee8`(十段부터 거꾸로), DP `0x1c636c`, 裏八段 `0x1c6768`·裏九段 `0x1c67ac` (68바이트 레코드, 아래) |
| 8th style | SP `0x199750`(7級~八段)·`0x199cd0`(十段), DP `0x199d20` (16바이트 레코드, 아래) |
| 9th style | SP `0xb5540`(7級~八段)·`0xb59b0`(十段), DP `0xb59f0` |
| 10th style | SP `0xc6790`, DP `0xc6bd0` (형식이 다름, 아래) |
| 11 IIDX RED | SP `0xe5dc0`, DP `0xe6200` |
| 12 HAPPY SKY | SP `0xfd9c0`, DP `0xfde30` |
| 13 DistorteD | SP `0x10af90`, DP `0x10b410` |
| 14 GOLD | `0x145250` |
| 15 DJ TROOPERS | `0x15e200` |
| 16 EMPRESS | `0x167fe0` |
| 16 PREMIUM BEST | `0x16c530` (쓰지 않음, 아래) |

- 형식: (곡 번호, 차트) 쌍 144개. 각각 32비트 두 개이고, 곡 번호는 곡 표의 레코드 순번(`Song::id`)이다.
- 차트 값: 0 NORMAL, 1 HYPER, 2 ANOTHER, 3 BEGINNER, 4 BLACK. BLACK 차트가 있는 곡에만 4가 나온다(EMPRESS KUNG-FU EMPIRE, DJ TROOPERS UBERTREFFEN 등).
- 4쌍이 한 단위다. 앞의 18단위가 SP, 뒤의 18단위가 DP이고, 순서는 7級 … 1級, 初段 … 十段, 皆伝이다.
- `(0, 0)`은 빈 칸이다.
  - DP의 7級·6級 두 단위는 통째로 비어 있다.
  - DP 단위는 넷째 칸이 비어 있어서 3곡짜리다. 네 에디션 모두 같다.
- 근거:
  - 단위 순서대로 레벨이 오른다.
  - EMPRESS SP 皆伝은 NAGEKI NO KI·3Y3S·HIMIKO·MEI(冥)이고 모두 ANOTHER다.
  - 네 에디션에서 같은 모양의 표가 하나씩만 나온다.
  - 실행 파일 문자열에 모드 이름 `STANDARD EXPERT CLASS`가 있다(EMPRESS).
  - 단위 이름(級·段)은 실행 파일에 텍스트로 없다. 순서는 표의 위치에서 정했다.
- PREMIUM BEST 디스크에도 같은 표(와 아래 EXPERT 표)가 자기 곡으로 채워져 있다. 하지만 이 디스크에는 STANDARD·EXPERT·CLASS 모드가 없어서(RemyWiki `CS_EMPRESS`) 플레이어는 읽지 않는다.
- 14·15·16의 단위 곡 구성은 RemyWiki `Beatmania_IIDX_CS_Dan_Courses`와 모두 일치한다(DP 7級·6級 없음, DP 3곡 포함).
- 찾은 방법: 곡 번호로 읽히는 값이 이어지는 구간을 실행 파일 전체에서 훑었다(`crates/cs-disc/examples/exe_dump.rs`로 실행 파일과 곡 목록을 꺼냄).
- 10th~13(`DanTable::Listed`):
  - 코스마다 (곡 번호, 차트, 칸 비트 1·2·4·8) 32비트 세 개짜리가 이어지고 -1로 끝난다. 다음 코스는 16바이트 경계에서 시작한다.
  - SP는 7級부터, DP는 5級부터다(DP는 3곡). 차트 값은 14 이후와 같다.
  - 13은 皆伝까지 SP 18·DP 16개, 10th~12는 皆伝이 없어서 十段까지 SP 17·DP 15개다.
  - 12 HAPPY SKY는 SP 十段 뒤에 곡 2개(SCRIPTED CONNECTION HYPER, 冥 ANOTHER)짜리 레코드가 하나 더 있고, 그 뒤 빈 공간이 정렬 규칙보다 길다. 게임에 皆伝이 없으므로 쓰지 않는 자리로 보고, DP 시작 위치를 따로 둔다.
  - RemyWiki 단위 목록에서 SP 7級의 곡 순서를 검색해 찾았고, 네 에디션 모두 위키와 일치한다.
- 9th style(`DanTable::Listed`, 구간 셋):
  - 레코드는 10th와 같다. 곡 레코드 위치·파일 표만 다르다(`NINTH_STYLE`).
  - SP 九段은 13곡 풀에서 4곡을 뽑는 단위라(`0xb58f0`, 칸 비트가 1·2·4·8이 아님) 고정 코스로 만들 수 없어 읽지 않는다. 그래서 SP는 7級~八段 15개와 十段 하나, DP는 5級~十段 15개다. RemyWiki 단위 목록과 곡 구성이 모두 같다.
- 7th style(`ListForm::Fixed`, 단위인정이 처음 들어간 버전): 코스마다 68바이트에 (`u16 곡, u16 차트, u16`) 6바이트가 5번, SP는 앞 4개, DP는 앞 3개를 쓴다. 곡 번호 0(TOMORROW PERFUME, SP 六段)도 곡이라 빈 칸 표시가 없다. 순서는 SP 十段→7級, DP 十段→5級, 그다음 SP 전용 裏八段·裏九段. 차트 값은 8th와 같다. 위키 목록과 대부분 같지만 몇 곡이 다르다(예: 二段 3곡째가 위키는 THE EARTH LIGHT, 디스크는 ENTRANCE). 디스크를 따른다.
- 8th style(`ListForm::Records`): 곡마다 16바이트(`u16 곡, u16 차트`, 모르는 8바이트, 칸 플래그 4바이트), 곡 0xffff 항목으로 끝난다. 차트 값은 0 HYPER, 1 NORMAL, 3 ANOTHER, 4 BEGINNER. SP 九段은 12곡 무작위 풀(`0x199c00`)이라 읽지 않고, DP 十段 뒤의 코스 하나(`0x19a0e0`)는 게임에 없다. 위키 단위 목록과 모두 같다.
- 차트 값의 HYPER·NORMAL: 9th·10th·11은 0이 HYPER, 1이 NORMAL이다(`hyper_first`). 12부터 0 NORMAL, 1 HYPER. 고단위(七段 등)의 0번 차트가 레벨상 HYPER인 것으로 확인했다(10th 六段 LOVE IS ETERNITY 0 → HYPER 8, NORMAL 6). 전에는 10th·11에서 둘을 바꿔 읽고 있었다.
- 모르는 것:
  - 게임이 단위에서 허용하는 배치 옵션. 플레이어는 미러까지 허용한다고 가정한다(`GradeMirror`).
  - 원작 단위 게이지의 수치. 플레이어는 beatoraja CLASS 게이지로 친다.
## 8. EXPERT 코스 표

단위 표 바로 앞에 있다(`Layout::expert`, `read_expert_courses`). 코스 하나는 64바이트다.

- 0x00: 코스 이름 문자열의 주소(ELF 가상 주소, ASCII).
- 0x04부터: (곡 번호, 1, 1) 32비트 세 개짜리가 5번. 두 1의 뜻은 모른다.
- 차트 난이도는 표에 없다. 게임에서 플레이어가 고른다.

| 에디션 | 위치 | 코스 수 |
|---|---|---|
| 3rd style | `0x7a780` | 16 (SOUL … HELL), 20바이트. 둘째 코스는 위키에 이름이 없어 `#2` |
| 4th style | `0x13f5c8` | 15 (VOCAL … L.E.D.), 5th와 같은 20바이트 형식 |
| 5th style | `0x18ebb8` | 18 (HEAVEN … L.E.D.), 20바이트(`u16` 곡, `u16` 차트 ×5), 이름 없음 |
| 6th style | `0x191da4` | 31 (LOUNGE … 日 5th style), 96바이트, 이름 없음. 위키에 없는 3개(10~12번째)는 `#10`~`#12` |
| 7th style | `0x1c59dc` | 9 (MORNING … SELECTION), 68바이트, 이름 없음 |
| 8th style | `0x1a7e18` | 17 (FEEL … 7-SP10D), 이름 없음(아래) |
| 9th style | `0xb6e08` | 17 (STYLISH … ASSAULT), 10th와 같은 형식 |
| 10th style | `0xc7fb8` | 17 (INSTRUMENTAL … KING), 형식이 다름 |
| 11 IIDX RED | `0xe7328` | 17 (J-POP … CORE) |
| 12 HAPPY SKY | `0xfefd8` | 17 (FESTA … TEMPEST) |
| 13 DistorteD | `0x10c558` | 17 (POPS … CARDINAL) |
| 14 GOLD | `0x144cc0` | 16 (POP … GAZILLIONAIRE) |
| 15 DJ TROOPERS | `0x15dc70` | 16 (FEELINGS … PANZERN) |
| 16 EMPRESS | `0x167b10` | 10 (FINE TUNE … EMPRESS) |
| 16 PREMIUM BEST | `0x16c120` | 10 (쓰지 않음, 7절) |

- 찾은 방법: RemyWiki `CS_EMPRESS`의 EXPERT 코스 목록(곡 순서)을 실행 파일에서 간격을 바꿔 가며 찾았다. 12바이트 간격으로 나왔고, 그 레코드 모양(이름 포인터 + 곡 5개)으로 다른 에디션도 찾았다.
- EMPRESS 10개 코스는 이름과 곡 순서가 위키와 모두 같다.
- 8th(`ExpertTable::Unnamed`): 72바이트 레코드에 (`u16` 곡, 차트를 정하는 `u16` 둘) 6바이트가 5번. 코스 이름은 그림뿐이라 RemyWiki `CS_8th_style`의 이름을 순서대로 붙인다. HELL 3곡째는 위키의 era (step mix)가 아니라 ERA (NOSTALMIX)다.
- 9th~13(`ExpertTable::Inline`): 9th는 곡 뒤 세 워드가 1이 아니라 0이나 1이다(뜻 모름, 위키의 LIGHT·ANOTHER 지정으로 보임).
- 10th~13(`ExpertTable::Inline`): 코스 하나가 124바이트다. 이름이 0x00부터 ASCII로 직접 들어 있고(포인터가 아님), 0x24부터 (곡 번호, 1, 1, 1) 16바이트가 5번 온다. 0x74의 두 워드는 코스 번호(1, 1 … 17, 17)다. 17개 뒤에 이름이 `ABCDEFGH`·`IJKLMNOP`인 더미 코스가 있어서 개수(17)로 끊는다. 위키의 CARDINAL GATE는 표에서 `CARDINAL`, RED의 ACCESSION은 `CS`, HAPPY SKY의 Ryu☆는 `Ryu`다. 네 에디션 모두 위키 CS 페이지와 곡 순서가 같다.
- 플레이어는 SP·DP × NORMAL·HYPER·ANOTHER 가운데 다섯 곡 모두에 차트가 있는 조합마다 코스를 하나씩 만든다.
- 앞서 실행 파일 밖(`ROMRS.DAT`, `ROMRIVAL.DAT`, `DATA1.DAT`)도 찾아봤다. `ROMRS.DAT`는 녹화된 플레이로 보이고, `ROMRIVAL.DAT`는 압축된 라이벌 데이터로 보인다.

## 9. 알 수 없는 것

- 배경음 팬 슬롯 0의 뜻, SATELLITE의 `c2` = 4.
- 명령 0xb(GOLD).
- 키음 엔트리 channel 0x0200–0x0700(GOLD, DJ TROOPERS).
- 판정 범위 여섯 값의 정확한 의미와 부호.
- 게임이 키음 음량을 선형으로 쓰는지, 그리고 콘솔 믹서의 실제 여유분.
