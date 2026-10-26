# IIDX16 STAGEFILE 추출 정리

곡 로딩 카드(장르·곡 제목·아티스트가 적힌 이미지)를 디스크에서 뽑아 BMS `#STAGEFILE`로 연결한 과정을 정리한다.

## 1. 원본

- 디스크 이미지: `<ISO>\SLPM_552.21.IIDX16 EMPRESS Disc1.iso`, `<ISO>\SLPM_552.22.IIDX16 EMPRESS Disc2.iso`
- Disc1 = EMPRESS, Disc2 = PREMIUM BEST
- ISO에서 필요한 파일만 복사했다. 작업 폴더: `<iidx16_extract>\disc1`, `<iidx16_extract>\disc2`
  - `SLPM_552.2x`(실행 파일), `DATA1.DAT`, `BM2DX16A/B/C.DAT`, `ROMRIVAL.DAT`, `ROMRS.DAT`

## 2. 구조 파악

### 2.1 실행 파일의 곡 테이블

| 디스크 | 실행 파일 | 곡 테이블 | 레코드 크기 |
|---|---|---|---|
| Disc2 (PREMIUM BEST) | `SLPM_552.22` | `0x17e0f0` | `0x134` |
| Disc1 (EMPRESS) | `SLPM_552.21` | `0x178bf0` | `0x134` |

- 레코드 첫 0x20바이트가 곡 이름(NUL 종료)이다.
- Disc2는 레코드 0이 ANISAKIS(0000번), 65번이 ZERO-ONE이다.

### 2.2 DATA1.DAT 파일 테이블

- 실행 파일에 16바이트 레코드 테이블이 있다: `(fileid, offset, size, _)`. offset/size는 0x800 단위다.
  - Disc2: `0x139980`, Disc1: `0x139d00`
- 각 항목은 "컨테이너"다. 헤더와 파일 목록이 들어 있다.
  - `u16 data_offset`, `u16 count`, 이어서 `(압축 해제 크기, 압축 크기)` 쌍 `count+1`개
  - 파일 데이터는 `data_offset`부터 연속으로 있고, 각 파일은 `sprites.decode_lzss`로 푼다.
- 첫 번째 파일(0번)은 애니메이션/헤더 데이터, 나머지는 텍스처다.

### 2.3 텍스처 형식

- PS2 PSMT8(8bpp 인덱스 + 256색 CLUT)
- 파일 앞 0x400바이트 = CLUT(256 × RGBA u32), 그 뒤가 `w × h` 인덱스 데이터
- 크기는 압축 해제 크기에서 역산한다: `w × h = 압축해제크기 − 0x400`
  - 예: `0x50400` → 640×512, `0x64400` → 640×640, `0x24400` → 384×384

## 3. 로딩 카드 찾기

### 3.1 Disc2 (PREMIUM BEST)

- 곡 레코드에는 `(인덱스, 컨테이너 번호)` 쌍이 여러 개 있다. 컨테이너 번호는 `0x5xx` 범위다.
- 로딩 카드는 **인덱스 0 쌍 중 UI 공용 컨테이너(0x520~0x523, 0x6a6, 0x5a8)를 뺀 마지막 것**이 가리키는 컨테이너의 **0번 텍스처**다.
  - 예: ANISAKIS 레코드의 `(0, 0x538)` → 컨테이너 0x538 텍스처 0 = 로딩 카드
- 검증: ANISAKIS, ZERO-ONE 카드가 실제 화면과 같았다.

### 3.2 Disc1 (EMPRESS)

- 레코드 구조가 Disc2와 달라서 위 규칙이 맞지 않았다. 곡 → 컨테이너 연결을 자동으로 찾지 못했다.
- 대신 **384×384 텍스처를 가진 컨테이너 99개**(313~411번)를 찾았다. 이 컨테이너들이 카드다.
- 각 카드에 적힌 제목을 눈으로 읽어 곡 테이블 이름과 맞췄다(`<iidx16_extract>\tools\empress_map.py`).
  - 예외: TECHNO BANCHO = 357번(한자 문구로 판단, 흐릿해서 불확실), NO CRIME J = NO CRIME(346번)과 같은 카드로 처리

## 4. 이미지 정규화

- 크기: **640×480 PNG**. wcko87의 BMS 가이드(BMS Overview, Production Checklist)가 STAGEFILE을 640×480 PNG로 권장한다. 공식 명세에서는 확인하지 못했다.
- 방법
  1. 카드를 비율을 유지하며 640×480 안에 들어가도록 키운다(보통 384→480).
  2. 밝기 40 초과 픽셀의 경계 상자(bbox)를 구한다.
  3. bbox 중심이 캔버스 중앙(320, 240)에 오도록 옮긴다. 내용이 캔버스 밖으로 잘리지 않도록 이동 범위를 제한한다.
  4. 검은 배경으로 채운다.
- 원본 카드는 자르지 않는다. 잘라서 생긴 문제(아티스트 이름 "HHH" 등이 잘림)가 있었다.

## 5. BMS 적용

- 곡 폴더마다 `stagefile.png`를 넣는다.
- 각 `.bms`의 `#TITLE` 줄 바로 다음에 `#STAGEFILE stagefile.png`를 넣는다. 기존 `#STAGEFILE` 줄은 지운다.
- 줄바꿈(CRLF)은 원본 그대로 유지한다. 인코딩은 바이트 단위로 다루어서 깨지지 않는다.
- 수정 전 원본은 `<iidx16_extract>\bms_backup\<PREMIUM BEST|EMPRESS>\<곡 폴더>\`에 백업했다.
- 결과
  - PREMIUM BEST: 곡 99개, BMS 707개
  - EMPRESS: 곡 100개, BMS 732개(100~103번 폴더는 제외)

## 6. 산출물

| 위치 | 내용 |
|---|---|
| `<iidx16_extract>\title\card_v2\` | PREMIUM BEST 카드 원본 99장 |
| `<iidx16_extract>\title\stagefile_centered\` | PREMIUM BEST 중앙 정렬 640×480 |
| `<iidx16_extract>\title\empress_cards\` | EMPRESS 카드 원본 100장 |
| `<iidx16_extract>\title\empress_stagefile\` | EMPRESS 중앙 정렬 640×480 |
| `<iidx16_extract>\title\*_log.json` 등 | 곡별 처리 기록 |

## 7. 사용한 스크립트/도구

- `<iidx-ps2tools>\animtool\imageops.py`: 원래 Cython 모듈(`imageops.pyx`)을 빌드할 수 없어서 만든 순수 Python 대체 모듈. 블렌딩 함수 두 개를 옮겼다. 이번 추출에서는 직접 쓰지 않았다.
- `<iidx16_extract>\tools\extract_title_art.py`: Disc2 곡 레코드 → 컨테이너 → 텍스처 추출 스크립트. 카드 규칙은 3.1과 같다.
- `<iidx16_extract>\tools\empress_cards.py`: Disc1 카드 추출과 중앙 정렬 함수(`center_640x480`)
- `<iidx16_extract>\tools\empress_map.py`: EMPRESS 카드 번호 → 곡 제목 매핑 표

재현 예(bash):

```bash
cd "<iidx16_extract>/tools"
python3 empress_cards.py
```

Python 패키지: `pillow`, `audioop-lts`, `pydub`, `blowfish`, `hexdump`, `capstone`(분석용).

## 8. 참고와 한계

- PS2 실행 파일의 레코드 필드 의미는 일부만 해석했다. Disc1 레코드는 Disc2 규칙이 맞지 않았다.
- 카드 대조는 사람이 이미지를 보고 했다. 제목이 흐리거나 한자인 곡은 틀렸을 수 있다.
- STAGEFILE 640×480은 커뮤니티 가이드 기준이다. 플레이어(beatoraja, LR2 등)에 따라 권장 크기가 다를 수 있다.
- 컨테이너 번호가 곡 순서와 일치하지 않아서, 다른 버전 디스크에 같은 방법을 적용하려면 카드 대조를 다시 해야 한다.
