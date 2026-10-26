# IIDX 디스크 조사 기록 (CS, PS2)

PS2 IIDX 디스크(CS)를 읽으면서 쌓은 조사 기록을 모은 곳이다. 흩어져 있던 작업 메모를 옮겼고, 로컬 경로는 `<iidx16_extract>`, `<iidx-ps2tools>`, `<ISO>`, `<root>`처럼 자리표시자로 바꿨다.

디스크 형식의 현재 기준은 코드와 [iidx_ps2_disc.md](../iidx_ps2_disc.md), [popn_ps2_disc.md](../popn_ps2_disc.md)다. 이 기록은 그 아래 근거다. 기록 시점(2026-10-23 전후) 이후 바뀐 내용은 코드가 맞다.

## 문서

| 문서 | 내용 |
| --- | --- |
| [chart_binary_guide.md](chart_binary_guide.md) | `.ply` 채보 바이너리 구조와 이벤트 해석 |
| [disc2_texture_findings.md](disc2_texture_findings.md) | PREMIUM BEST 디스크의 텍스처·문자열 조사(아티스트, 장르, 제목은 이미지로 들어 있음) |
| [stagefile_extraction.md](stagefile_extraction.md) | 로딩 카드(스테이지 아트) 추출과 BMS `#STAGEFILE` 연결 |
| [song_db_mapping_plan.md](song_db_mapping_plan.md) | 외부 곡 DB(Textage 기반)와 디스크 곡 매핑 계획과 결과 |

이전 정리에 있던 `disc_direct_playback_plan`(DVD 직접 재생 계획)은 [docs/plans/2026-10-23-disc-direct-playback.md](../../plans/2026-10-23-disc-direct-playback.md)와 내용이 겹치므로 옮기지 않았다. 전체 요약(`project_summary`)은 이 README로 대신한다.

## 확인된 것 (요약)

- 채보는 `.ply`이고, 이벤트 구조는 7th 스타일 PS2 채보와 같다.
- 곡 제목(ASCII)은 실행 파일의 곡 테이블에 있다. 아티스트, 장르, 일본어 제목은 텍스처로만 들어 있어서, 외부 DB와 매핑해 채운다.
- 로딩 카드(스테이지 아트)는 DATA1 컨테이너 안에 있고, 앱은 이것을 디스크에서 직접 읽는다.
- 외부 DB(tts1374/iidx_all_songs_master)에는 CS 곡 정보가 없다. AC 곡만 들어 있을 가능성이 크다.

## 작업 데이터 위치

추출 결과물(텍스처 PNG, 원본 바이너리, 임시 도구 출력)은 저장소에 넣지 않았다. 수백 MB 단위이고 디스크 이미지에서 다시 만들 수 있다. 위 문서가 만드는 방법을 적고 있다.

## 아직 남은 것

- 아티스트·장르 텍스처와 곡 레코드의 매핑은 일부 곡에서만 확인됐다(`disc2_texture_findings.md`).
- 일부 CS 키음은 디스크 데이터에서 무음으로 들어 있다. 재생 결과가 실제 게임과 다를 수 있는 부분이라 별도로 기록한다.
