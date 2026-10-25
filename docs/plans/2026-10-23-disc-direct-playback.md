# PS2 IIDX 디스크 직접 플레이 설계 (초안)

작성: 2026-10-23. 입력 문서: `../iidx16-bms/docs/disc_direct_playback_plan.md` (저장소 밖, 별도 작업 폴더).

## 1. 목표

- beatmania IIDX 16 EMPRESS / PREMIUM BEST (PS2) ISO를 곡 목록에 올리고, 설치된 BMS처럼 고르고 플레이한다.
- `.bmsp`나 곡 폴더를 미리 만들지 않는다. 디스크에서 읽은 데이터는 메모리에만 둔다(개인 용도, 재배포 없음).

## 2. 현재 beetle 구조에서 확인한 사실

| 항목 | 현재 | 디스크 소스에 주는 영향 |
|---|---|---|
| bpm "플러그인" | **없음.** bpm/beetle에 소스 플러그인 인터페이스는 존재하지 않는다 | 계획 문서 11절의 첫 질문에 대한 답. 인터페이스를 새로 만들어야 한다 |
| 곡 출처 | `beetle-app/src/scanner.rs`: `library.dat` 폴더 + 레지스트리 `.bmsp` 두 종류 | 세 번째 출처(디스크)를 추가 |
| 곡 주소 | `SongMetadata::file_path` 문자열. `pkg::entry`면 패키지, 아니면 폴더 경로 | 디스크용 주소 형식 필요 |
| 로딩 | `loader.rs::load_chart_and_audio_with_seed`가 `split_once("::")`로 분기, 패키지/폴더 경로를 각각 통째로 구현(약 350줄 중복) | 세 번째 분기를 복붙하면 안 된다 → 먼저 소스 추상화 |
| 차트 식별 | `ChartId` = 차트 바이트의 SHA-256, `md5` 별칭 | 디스크 채보를 **결정론적으로 BMS 텍스트로 합성**하면 기록·리플레이·난이도표 매칭이 그대로 동작 |
| 키음 | `SampleBank::insert(WavId, PcmBuffer)`, 버퍼마다 `sample_rate` 보유 | ADPCM 디코딩 결과(24000Hz / 44100Hz)를 OGG를 거치지 않고 바로 넣을 수 있다 (INV-3 충족) |

## 3. 핵심 결정: "BMS 텍스트 합성 + PCM 직접 공급"

디스크 채보(.ply)를 beetle 내부 모델(`BmsChart`)로 바로 바꾸는 대신, **메모리 안에서 BMS 텍스트를 만든다.**

- 장점: 파서·`#RANDOM` 없음·타이밍 모델·`SongMetadata::from_bytes`·`ChartId`·판정이 전부 기존 코드. 디버깅 시 합성 텍스트를 파일로 떨궈 기존 BMS 결과(`IIDX16_charts`)와 diff 가능.
- 조건: 합성은 바이트 단위로 결정론적이어야 한다(같은 디스크 → 같은 `ChartId`). 정렬된 출력, 고정 소수 형식.
- `#WAVxx` 이름은 가상 이름(`0001.wav` …)으로 두고, 로더가 디스크 소스에서 이름으로 PCM을 받는다.
- 시간: `.ply` 시각은 59.94fps **프레임** 단위다(배율 0x412B = 16.683ms). 게임은 가장 가까운 프레임으로 반올림한다(두 디스크의 마디선 잔차가 ±0.5프레임에 고르게 분포). 그래서 마디선 시각이 기준이고, 저장된 BPM(정수, 표시용)은 비율로만 쓴다:
  - 같은 BPM으로 이어지는 마디들은 마디선에 맞춘 최소제곱 직선으로 시작 시각과 템포를 정한다(반올림된 BPM의 누적 오차 제거).
  - 마디 안에서 템포가 바뀌면 바뀌는 위치를 박자 격자에 맞추고, 각 구간 템포를 그 구간의 실제 프레임 길이로 다시 계산한다(SAKURA의 117→13 BPM 감속).
  - 노트·배경음 위치는 반 프레임 안에 드는 가장 거친 격자(1/1 … 1/48박)에 스냅한다.
- 확인된 형식 사실: BPM 이벤트의 상위 니블이 BPM 상위 바이트(SAKURA 300 = slot 1, c2 44). DP 채보는 마디선이 양쪽에 하나씩 같은 시각에 있다. 시각 0의 노트(채보마다 1~12개)는 자리표시라 연주하지 않는다(BMS 변환본과의 노트 수 차이 원인).
- 검증(`crates/cs-disc/tests/real_discs.rs`, ISO 경로 환경 변수가 있을 때만): 두 디스크 모든 채보(BEGINNER/BLACK 포함)가 beetle 파서로 읽히고 노트 수가 `.ply`와 같으며, 모든 노트가 디스크 시각에서 1프레임 안(최대 13.4ms)이다. 복호화·LZ 결과는 iidx-ps2tools가 뽑은 `.ply`와 바이트 단위로 같다.

## 4. 구조

```
crates/cs-disc (신규, 의존성 없음 — IIDX뿐 아니라 pop'n·DDR 등 CS 디스크 공용)
  sector.rs    2048바이트 섹터 단위 읽기: ISO 파일과 광학 드라이브(\\.\E:)를 한 경로로  [완료]
  drive.rs     CD/DVD 드라이브 목록(GetDriveTypeW), 드라이브 열기                       [완료]
  iso9660.rs   ISO 디렉터리/파일 범위 읽기                                             [완료]
  ps2.rs       SYSTEM.CNF의 BOOT2 → 실행 파일 이름(제품 코드)으로 게임 판별             [완료]
  iidx16.rs    곡 테이블·레벨·채보 파일 번호, BM2DX16{A,B,C}.DAT 파일 테이블            [완료]
  blowfish.rs  .ply 복호화 (직접 구현, CBC-CTS, iidx-ps2tools와 바이트 일치)              [완료]
  lz.rs        PS2 IIDX LZ 해제                                                        [완료]
  ply.rs       7th 스타일 채보 → 이벤트                                                 [완료]
  ply_bms.rs   이벤트 → BMS 텍스트 (결정론적)                                           [완료]
  (이후 게임마다 모듈 하나: popn*.rs, ddr*.rs …)
  adpcm.rs     PS2 SPU ADPCM, 프레임 스크램블 해제 (vgmstream과 샘플 단위로 일치)          [완료]
  wvb.rs       키음 뱅크(.wvb)·BGM 스트림(.pcm) → 16비트 PCM                              [완료]
  ps2tex.rs    로딩 카드: LZSS, PSMT8 스위즐·CLUT → RGBA, 4:3 중앙 정렬 (iidx16-bms 추출본과 픽셀 일치) [완료]
  meta.rs      곡 세부 정보 표(TSV) 읽기: 제목·부제·아티스트·장르·카드 컨테이너              [완료]

beetle-core   ChartSource 개념 없음(순수 유지). 주소 파싱만 추가 가능.
beetle-app    loader.rs: SongSource 추상화 → Folder / Package / Disc
              scanner.rs: disc 출처 스캔(디스크 항목을 SongMetadata로)
bms-package-manager: library.dat에 .iso 등록(`bpm library add x.iso`), bpm-gui 표시
```

주소 형식: `disc:<이미지 경로 또는 드라이브>::<곡 번호>/<채보>` (`beetle_core::SongAddress`). 기존 `pkg::entry` 판별과 겹치지 않도록 접두사로 먼저 가른다. 드라이브는 드라이브 문자(`disc:E:::…`)로 두고, 디스크를 바꿔 넣는 경우는 열 때 게임 판별로 다시 확인한다.

## 5. 먼저 고칠 것 (순서)

1. **beetle: 로더의 출처 추상화 (디스크와 무관하게 이득)**
   `load_chart_and_audio_with_seed`와 `load_stage_image`의 폴더/패키지 분기를 `SongSource` 하나로 정리한다.
   대략: `chart_bytes()`, `sound(name) -> Option<PcmBuffer 또는 bytes>`, `image(name)`, `stage_image()`, `videos()`.
   패키지의 사운드 아틀라스 같은 빠른 경로는 소스 구현 안에 둔다. 동작 변화 없는 리팩터 + 기존 로더 테스트로 확인.
2. **beetle: 스캐너에 세 번째 출처 자리**
   `scanner.rs`의 출처 종류(`Folder`, `Package`)에 `Disc`를 넣고 `choose_load_index` 순서(폴더 → 패키지 → 디스크)를 정한다.
   bpm 쪽 컬렉션 표시도 같은 순서를 써야 한다(`load_location.rs` 주석의 약속).
3. **cs-disc 크레이트** — 완료: ISO·드라이브 읽기, 게임 판별, IIDX16 곡 테이블, `.ply` 복호화·LZ·파싱, BMS 합성(`iidx16::Disc::chart_bms`). 이제 스캐너가 디스크 곡을 목록에 올릴 수 있다(다음: 스캐너 `Disc` 종류와 로더 `DiscSource`, 키음 없이 무음 플레이부터).
4. **음원** — 완료(`cs-disc`): `adpcm.rs`, `wvb.rs`, 채보별 뱅크 선택(`iidx16::Song::sound_files`).
   - 째지는 소리의 원인: Python 디코더가 (1) 시프트 12 초과 프레임을 무음으로 만들고 (2) 예측값을 내림했다. vgmstream은 시프트 12 초과를 9로 읽고, 합을 0 쪽으로 버린다. 이렇게 맞추니 키음 1,043개와 BGM 3곡이 vgmstream 출력과 샘플 단위로 같다(`examples/sound_check.rs`).
   - 실제 디스크 검증: 두 디스크 모든 채보가 쓰는 소리(1번 제외)가 자기 뱅크에 있고 디코딩되며, 모든 BGM이 디코딩된다.
   - 뱅크 항목: 왼쪽(0x0000) 다음 오른쪽(0x0100)이면 스테레오. 오른쪽 항목도 따로 하나의 소리로 남는다(DXY! 42, GEKKO 94를 채보가 직접 부름).
   - 볼륨·팬(항목 8, 9바이트)은 적용하지 않는다. 대부분 `01 7f`인데 문서대로 팬으로 읽으면 왼쪽 끝이 되어 의미가 확실하지 않다. 지금까지 들어 본 OGG 추출본과 같은 방식(저장된 그대로)이다.
5. **메타/스테이지 이미지** — 완료. 표의 형식, 초안 만들기, 사람이 채우는 방법은 [disc_meta.md](../specs/disc_meta.md).
   - 곡 세부 정보는 사용자 쪽 파일 `disc_meta/<에디션>.tsv`(`iidx16-empress`, `iidx16-premium-best`)에 둔다. Textage에서 온 데이터라 저장소에는 넣지 않고(`.gitignore`), `scripts/disc_meta_from_textage.py`가 iidx16-bms의 `song_map_final.csv`·`titletbl.js`·`empress_map.py`로 초안을 만든다. `song_map_final.csv`의 모든 행은 확인된 매핑이다(제목+레벨로 자동 매칭되지 않은 곡은 그 CSV에서 사람이 맞췄다). 매칭 방식은 참고용으로 `note`에 남기고, 카드가 없는 EMPRESS 곡만 `CHECK`로 표시한다. 빈 칸은 디스크 값(ASCII 제목)을 쓴다.
   - 채보 식별값(`ChartId`)은 세부 정보 없이 디스크만으로 만든 BMS에서 정한다. 표를 고쳐도 기록이 끊기지 않는다. 표가 곡 캐시보다 새로우면 다시 스캔한다.
   - 로딩 카드: PREMIUM BEST는 곡 레코드 규칙(마지막 `(0, 0x5xx~0x6xx)` 쌍, 공용 UI 컨테이너 제외), EMPRESS는 규칙이 맞지 않아 표의 `card` 열(사람이 카드를 보고 맞춘 `empress_map.py`)을 쓴다. 두 디스크 199곡의 카드가 Python 추출본과 픽셀 단위로 같다(`examples/card_dump.rs`). (2026-10-25: 두 디스크 모두 레코드 0x94가 카드 번호임을 알아내어 규칙과 `card` 열을 없앴다. `docs/specs/iidx_ps2_disc.md` 6절.)

**게임 연결 (완료)**: `library.dat`에 `.iso` 경로나 드라이브(`E:`)를 넣으면(`bpm library add <x.iso | E:>`) 스캐너가 디스크의 모든 채보를 `Disc` 종류로 올리고(폴더 → 패키지 → 디스크 순, 디스크 두 장 약 2.9초), 로더의 `DiscSource`가 채보와 키음·BGM을 디스크에서 바로 읽는다(ANISAKIS ANOTHER: 키음 735개 0.43초). 명령줄로 `beetle-app "disc:<이미지>::000/sp_another"`처럼 바로 플레이할 수도 있다. BGA 영상: 곡 레코드 0x58의 영상 파일(MPEG-2 프로그램 스트림)을 그대로 넘기고, 자체 MPEG-1/2 디코더(`crates/mpeg-video`)가 재생한다(Windows의 MPEG-2 디코더는 Store 확장이라 기대하지 않음). 남은 것: 실제 귀로 싱크·음질 확인, bpm-gui와 `bpm scan`의 디스크 표시, 앱 안에서 ISO 추가(끌어 놓기·곡 폴더 설정), DVD 실물 확인.

## 5-1. 버전별 처리 (제품 코드 → 형식)

디스크를 열면 `SYSTEM.CNF`의 실행 파일 이름(제품 코드)으로 버전을 정한다(`crates/cs-disc/src/titles.rs`). 표에 있지만 `playable`이 아닌 디스크는 이름만 알아보고(곡 목록 로그, `disc_songs` 예제에 "not read yet"), 읽지는 않는다.

| 버전 | 제품 코드 | 채보 형식 (`ply::ChartFormat`) | 읽기 |
|---|---|---|---|
| 3rd, 4th style | SLPM_650.06, 650.26 | `Ps2NoHeader`: 헤더 없음, 4바이트 이벤트, 배율 0x414C | 아직 |
| 5th, 6th style | SLPM_650.49, 651.56 | `Ps2Short`: 헤더 + 4바이트 이벤트 | 아직 |
| 7th ~ 15 DJ TROOPERS | SLPM_655.93, 657.68, 659.46, 661.80, 664.26, 666.21, 668.28, 669.95, 551.17 | `Ps2Long`: 헤더 + 8바이트 이벤트(`.ply`) | 아직 |
| 16 EMPRESS / PREMIUM BEST | SLPM_552.21 / 552.22 | `Ps2Long` | 됨 |
| beatmania (US) | SLUS_212.39 | `Ps2Long` | 아직 |
| 아케이드 `.1` | (파일) | `Arcade`: 12개 디렉터리, 타입 바이트와 파라미터가 따로, BPM = 값/파라미터, 틱은 GOLD 전 59.94·GOLD 60.046·그 뒤 1000/초 (`ChartFormat::arcade(버전)`) | 파서만 |

- 근거: 제품 코드와 PS2 형식은 iidx-ps2tools(`plugins/*.py`의 `is_match`, `charttool/plugins/iidx*cs.py`), `.1`은 SaxxonPike `rhythm-game-formats/iidx/1.md`.
- 실물로 확인한 것은 16(`Ps2Long`)뿐이다. 나머지 형식은 위 근거와 단위 테스트까지다.
- 다른 버전을 읽으려면 채보 형식 외에 곡 테이블·파일 테이블 위치, 암호 키, 키음 형식도 버전마다 옮겨야 한다(iidx-ps2tools 플러그인별로 다름). `crates/cs-disc/src/iidx.rs`에 에디션과 `Layout`(표 위치, 레코드 배치, 키, 스테이지 그림 위치)을 하나 추가하고 표의 `playable`을 켠다.
- `.1`의 디렉터리 번호 ↔ 난이도 대응은 버전마다 다르다(iidx16-bms에서 Scharfrichter가 0/6을 HYPER로 읽는 차이를 겪음). 쓰게 되면 버전별로 정한다.
- `ply_bms`(BMS 합성)의 스냅 허용치는 PS2 프레임(반 프레임) 기준이다. 1000틱 `.1`을 합성하게 되면 허용치를 틱에 맞춘다.

## 6. 불변식 점검

- INV-3: 곡 로딩 때 해당 곡의 키음 전체를 디코딩해 `SampleBank`에 적재. 플레이 중 ISO 읽기 없음.
- INV-5: ISO 스캔·디코딩은 기존 백그라운드 로더/스캐너 스레드에서.
- 바이너리 크기: 새 의존성 0개. 메타 테이블 크기는 측정해 기록.
- 디스크 데이터를 디스크(로컬 파일)에 캐시하지 않는다. 다만 스캔 결과(`SongMetadata`)는 기존 곡 캐시에 들어간다 — 제목·레벨 정도이므로 허용으로 본다.

## 7. 열린 질문

- 디스크 1(EMPRESS)과 2의 음원·채보 형식이 완전히 같은지 (전수 확인 필요).
- BEGINNER/BLACK 채보: 포함함(`#DIFFICULTY` 1, 5).
- 노트 수 차이: 시각 0의 자리표시 노트 때문으로 확인(3절).
- `auto` 이벤트는 BGM 채널(`01`)로 둔다. 키음이 하나도 없는 노트는 `ZZ`(정의 없는 소리)로 둔다. 노트의 `param`(길이)은 두 디스크 전체에서 0이라 롱노트·BSS는 없다.
- 키음 번호 → 소리: 합성 BMS는 `#WAVxx NNNN.wav`(NNNN = 채보의 소리 번호)로 쓴다. 1번은 BGM 신호(채보 첫 `auto`)라 그 채보의 BGM 스트림(.pcm)을 낸다. 나머지는 채보의 키음 뱅크에서 같은 번호. 채보의 (뱅크, BGM)은 곡 레코드의 소리 쌍 8개 중 자기 칸, 없으면 DP HYPER(DP) → SP HYPER 순으로 빌린다(BLACK은 항상 빌림).
- 실제 광학 드라이브: 받기로 함. 드라이브는 섹터 경계 읽기만 허용하므로 모든 읽기를 섹터 단위로 넓혀 처리한다(`sector.rs`). 실제 DVD로는 아직 확인하지 못함(드라이브 목록은 잡히지만 디스크 없음).
- DVD 읽기 속도: 곡 하나 로딩에 키음 수 MB를 탐색 읽기하므로, 파일 번호 순으로 정렬해 읽는 등 탐색을 줄이는 방법을 음원 단계에서 확인.
