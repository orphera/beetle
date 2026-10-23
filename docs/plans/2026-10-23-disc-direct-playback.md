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
- 시간 단위는 ms(`BemaniToBMSTroopers` 방식)로 직접 계산한다. 59.94fps 경로는 쓰지 않는다.

## 4. 구조

```
crates/cs-disc (신규, 의존성 없음 — IIDX뿐 아니라 pop'n·DDR 등 CS 디스크 공용)
  sector.rs    2048바이트 섹터 단위 읽기: ISO 파일과 광학 드라이브(\\.\E:)를 한 경로로  [완료]
  drive.rs     CD/DVD 드라이브 목록(GetDriveTypeW), 드라이브 열기                       [완료]
  iso9660.rs   ISO 디렉터리/파일 범위 읽기                                             [완료]
  ps2.rs       SYSTEM.CNF의 BOOT2 → 실행 파일 이름(제품 코드)으로 게임 판별             [완료]
  iidx16.rs    곡 테이블·레벨·채보 파일 번호, BM2DX16{A,B,C}.DAT 파일 테이블            [완료]
  (이후 게임마다 모듈 하나: popn*.rs, ddr*.rs …; 아래 파일은 iidx16 하위로)
  blowfish.rs  .ply 복호화 (직접 구현, 크레이트 금지 정책)
  ply.rs       7th 채보 → 이벤트
  to_bms.rs    이벤트 → BMS 텍스트 (결정론적)
  wvb.rs, adpcm.rs  키음/BGM → f32 PCM
  meta.rs      구워 둔 메타 테이블(곡 번호 → 제목/부제/아티스트/장르, 수십 KB)

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
3. **cs-disc 크레이트: 읽기 전용 부분부터 Rust 포팅** — ISO·드라이브 읽기, 게임 판별, IIDX16 곡 테이블까지 완료(두 ISO에서 104곡·99곡 확인). 남은 것: `.ply` 복호화(Blowfish)·LZ 해제·파싱 → BMS 합성.
   ISO9660 → 곡 테이블/레벨 → `.ply` 복호화·파싱 → BMS 합성. 이 단계에서 **스캔(곡 목록 표시)까지** 가능하다.
   검증: 합성 BMS의 노트 수를 `IIDX16_charts` 1,170개 일치 결과와 비교하는 테스트(ISO 있을 때만 실행, `#[ignore]`).
4. **음원**: WVB + ADPCM 포팅. 계획 문서 5절의 음질 문제(째지는 소리)는 **Python에서 vgmstream과 샘플 비교로 먼저 해결**하고 포팅한다. 키음 볼륨/팬도 이때 넣는다.
5. **메타/스테이지 이미지**: `song_map_final.csv`에서 디스크별 테이블을 굽는다. 스테이지 이미지는 처음엔 없이 가고 나중에 결정.

1·2는 지금 beetle에서 바로 시작할 수 있다. 4는 iidx16-bms 쪽 조사가 먼저다.

## 6. 불변식 점검

- INV-3: 곡 로딩 때 해당 곡의 키음 전체를 디코딩해 `SampleBank`에 적재. 플레이 중 ISO 읽기 없음.
- INV-5: ISO 스캔·디코딩은 기존 백그라운드 로더/스캐너 스레드에서.
- 바이너리 크기: 새 의존성 0개. 메타 테이블 크기는 측정해 기록.
- 디스크 데이터를 디스크(로컬 파일)에 캐시하지 않는다. 다만 스캔 결과(`SongMetadata`)는 기존 곡 캐시에 들어간다 — 제목·레벨 정도이므로 허용으로 본다.

## 7. 열린 질문

- 디스크 1(EMPRESS)과 2의 음원·채보 형식이 완전히 같은지 (전수 확인 필요).
- BEGINNER/BLACK 채보: BMS 합성은 형식 제약이 없으므로 포함할지.
- `.ply` `note_p1` 634 vs 631, BMS 6개 초과의 원인 — 합성기 테스트로 같이 잡는다.
- `auto` 이벤트를 BGM 채널(`01`)로 둘지.
- 실제 광학 드라이브: 받기로 함. 드라이브는 섹터 경계 읽기만 허용하므로 모든 읽기를 섹터 단위로 넓혀 처리한다(`sector.rs`). 실제 DVD로는 아직 확인하지 못함(드라이브 목록은 잡히지만 디스크 없음).
- DVD 읽기 속도: 곡 하나 로딩에 키음 수 MB를 탐색 읽기하므로, 파일 번호 순으로 정렬해 읽는 등 탐색을 줄이는 방법을 음원 단계에서 확인.
