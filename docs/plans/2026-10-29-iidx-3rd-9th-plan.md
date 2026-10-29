# PS2 IIDX 3rd~9th style 디스크 읽기 계획

10th~16을 읽는 `crates/cs-disc`에 3rd~9th style을 더한다. 9th부터 3rd로 내려간다: 9th가 10th와 가장 가깝고, 아래로 갈수록 형식이 더 달라지기 때문이다. 근거는 iidx-ps2tools(`plugins/iidx<n>cs.py`, `charttool/plugins/iidx<n>cs.py`, `filetable_readers.py`)와 디스크 직접 집계. 알아낸 것은 [iidx_ps2_disc.md](../specs/iidx_ps2_disc.md)에 에디션별로 더한다.

## 에디션별 차이 (iidx-ps2tools 기준)

| 에디션 | 실행 파일 | 키음·채보 파일 | 파일 표 | 곡 레코드 | 채보 형식 |
|---|---|---|---|---|---|
| 9th | `SLPM_659.46` | `DATA2.DAT` (+ `DATA1.DAT`) | 8바이트(섹터, 섹터 수) `0xbd230` | 0x16c, 제목 인라인 `0xc1500` | Ps2Long (10th와 같음) |
| 8th | `SLPM_657.68` | `DX2_8/BM2DX8B/C/A.BIN` | 16바이트(섹터, ?, 바이트, 해시) | 0x9c, 제목은 문자열 포인터(`- 0xfff80`) | Ps2Long |
| 7th | `SLPM_655.93` | `DX2_7/BM2DX7B/C/A.BIN` | 16바이트 | 0xa0, 포인터 | Ps2Long |
| 6th | `SLPM_651.56` | `DX2_6/BM2DX6A/B.BIN` | 16바이트 | 0xa0 | Ps2Short |
| 5th | `SLPM_650.49` | `DX2_5/BM2DX5.BIN` | 16바이트 | 0xa4 | Ps2Short |
| 4th | `SLPM_650.26` | `DX2_4/BM2DX4.BIN` | 12바이트(섹터, 바이트, 해시) | 0x90 | Ps2NoHeader |
| 3rd | `SLPM_650.06` | `DX2_3/BM2DX3.BIN` | 12바이트 | 0x7c | Ps2NoHeader |

- 채보 파서(`ply.rs`의 세 `ChartFormat`)는 이미 있다. 새로 필요한 것은 에디션마다 `Layout`(표 위치, 레코드 배치)과 7th 이하의 포인터 제목·16/12바이트 파일 표·`.BIN` 파일 이름.
- 8th 이하에는 채보 암호가 없다(LZ만). 9th도 10th처럼 없다.
- 키음 형식(`.wvb`)과 BGM이 오래된 에디션에서도 같은지는 디스크마다 확인한다.

## 단위인정과 EXPERT

- 단위인정(段位認定)은 7th style CS에서 처음 들어왔다. 따라서 7th·8th·9th는 단위 표를 찾고, 3rd~6th는 `dan: None`이다. 7th~9th 표는 10th의 `DanTable::Listed`와 같은 모양일 가능성이 높다. RemyWiki `Beatmania_IIDX_CS_Dan_Courses`의 SP 7級 곡 순서로 실행 파일을 찾는다(10th 때와 같은 방법). 단위 수·皆伝 유무·DP 시작 단위는 에디션마다 위키와 맞춰 본다.
- EXPERT 코스는 에디션마다 있는지부터 확인한다(위키 CS 페이지). 있으면 `ExpertTable`에 형식을 더하고, 없으면 `None`.
- 앱(`beetle-app`의 `tables.rs`)은 디스크마다 코스가 있을 때만 코스 표를 보여 준다. 코스가 없는 디스크에서 빈 표가 생기지 않는지 확인한다.

## 단계

각 단계는 에디션 하나이고, 끝나면 커밋한다.

1. **9th** [완료]: `Edition::NinthStyle`, `Layout`(10th 레코드에서 레벨 위치·순서, 스테이지 그림 위치만 다름: 레벨 0x4a에 SP H/A, SP N, DP H/A, DP N, 메인 오버레이 0xac), 단위·EXPERT 표 찾기, `titles.rs`에서 `playable`.
   - 결과: 87곡, 단위 SP 16(九段은 무작위 풀이라 제외)·DP 15, EXPERT 17. 단위 표의 차트 값이 9th~11은 0 HYPER·1 NORMAL임을 알아내 10th·11의 단위 차트도 바로잡음. DP ANOTHER에 키음이 없으면 SP ANOTHER 것을 씀(GENOM SCREAMS).
2. **8th** [완료]: 포인터 제목, 16바이트 파일 표, 세 `.BIN`(B, C, A 순서), 0x9c 레코드. 단위 표.
   - 결과: 90곡, 단위 SP 16·DP 15(16바이트 레코드 형식, 차트 값 0 H·1 N·3 A·4 B), EXPERT 17(이름은 위키). 옛 키음 뱅크·BGM 형식, `DATA1` 없음, 분할 파일 번호, 마디 안 템포 변화의 마디 길이를 더함. 앱에서 디스크 곡·코스 표가 보이는 것까지 확인.
3. **7th** [완료]: 8th와 같은 틀에 0xa0 레코드. 단위 표(첫 단위인정).
   - 결과: 90곡, 단위 SP 17+裏 2·DP 15(68바이트 레코드, 十段부터 거꾸로), EXPERT 9(이름은 위키). MAX 300의 마디 안 템포 변화 위치 버그를 고쳐 모든 디스크 오차가 같거나 줄었음. 앱에서 디스크 곡·코스 표 확인.
4. **6th** [완료]: Ps2Short, 두 `.BIN`. 단위 없음.
   - 결과: 69곡(뒤의 자리 채움 레코드 제외), 채보는 실행 파일 안(LZ), 곡마다 A/B BIN, EXPERT 31(위키 이름 28 + 번호 3). 단위인정 없음. 앱에서 디스크 곡·코스 표 확인.
5. **5th**: `.BIN` 하나.
6. **4th**: 12바이트 파일 표, Ps2NoHeader.
7. **3rd**: 0x7c 레코드.

## 확인

- `crates/cs-disc/tests/real_discs.rs`(ISO 경로 환경 변수가 있을 때만)에 에디션마다: 모든 채보가 읽히고 노트 수가 iidx-ps2tools `.ply`와 같다, 곡 수와 제목이 위키 CS 곡 목록과 맞는다, 단위 코스 곡 구성이 위키와 같다.
- 앱에서 디스크를 열어 선곡·플레이·로딩 카드가 나오는지(곡 몇 개), 단위 코스가 있는 에디션은 코스 폴더가 나오는지.
- 스테이지 그림·배경 영상은 에디션마다 되는 만큼만 하고, 안 되는 것은 스펙 9절(알 수 없는 것)에 적는다.
