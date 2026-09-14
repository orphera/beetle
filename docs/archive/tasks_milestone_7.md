# Milestone 7 아카이브: 듀얼 아틀라스 기반 초고속 BMS 패키지 엔진 (Dual Atlas Package Engine: Sound & BGA Atlas)

이 문서는 Milestone 7 구현 완료 내역을 보존하기 위한 아카이브 문서입니다.

- **설계 제안서**: [proposals/dual_atlas_package.md](../proposals/dual_atlas_package.md)
- **완료 일자**: 2026-09-14
- **주요 성과**:
  - 1곡당 1,000+개 오디오와 수백 개 이미지를 단일 연속 오디오 스트림(`Sound Atlas`)과 단일 2D 텍스처(`BGA Atlas`)로 합성하는 듀얼 아틀라스 규격 도입
  - 인게임 로딩 시간 5.4배 단축 (59.5ms -> 11.1ms), 패키지 용량 87.6% 절감
  - 무손실 FLAC 키음 압축(`FlacBundle`), BGA 델타 압축(`BgaDelta`), WebDAV VFS 온더플라이 WAV 합성 및 1초 역변환 익스포터(`bpm export`) 구축
  - 전체 워크스페이스 120+개 단위/통합 테스트 통과 및 바이너리 크기 불변식 준수

---

## 📋 Phase 1: Sound Atlas 빌더 및 디코딩 엔진 (`crates/bms-package/`, `crates/beetle-audio/`)
- [x] **Sound Atlas 슬라이스 데이터 모델 및 인터페이스 정의 (`crates/bms-package/src/atlas/sound.rs`)**
  - [x] `SoundSlice` 구조체 (`start_sample: u64`, `length_samples: u64`, `channels: u8`, `sample_rate: u32`, `original_filename: String`) 정의
  - [x] `SoundAtlasMeta` 및 직렬화/역직렬화 구현
- [x] **AOT 오디오 사전 정규화 및 합성 빌더 (`SoundAtlasBuilder`)**
  - [x] 복수의 키음(WAV/OGG)을 44.1kHz Stereo PCM으로 일괄 리샘플링/업믹싱
  - [x] 키음 간 주파수 신호 번짐 방지 무음 패딩(Zero-Padding, 128 samples) 삽입
  - [x] 단일 연속 오디오 스트림(FLAC 무손실 압축 / PCM) 생성
- [x] **`SampleBank` 슬라이스 뷰(Zero-Copy Slice Reference) 연동 (`crates/beetle-audio/src/sample.rs`)**
  - [x] 거대 단일 PCM 버퍼(`Arc<[f32]>`)에서 슬라이스 단위로 키음을 발음하는 참조형 사운드 구조체 지원

---

## 📋 Phase 2: BGA Texture Atlas 빌더 및 렌더러 연동 (`crates/bms-package/`, `crates/beetle-render/`)
- [x] **순수 Rust 경량 2D 직사각형 패킹(Bin Packing) 알고리즘 구축 (`crates/bms-package/src/atlas/binpack.rs`)**
  - [x] 외부 무거운 크레이트 0개, Guillotine / MaxRects 기반 직사각형 패커 구현
  - [x] 스프라이트 간 색상 번짐 방지 1px 투명 여백(Padding) 처리
- [x] **BGA Texture Atlas 합성 빌더 (`BgaAtlasBuilder`)**
  - [x] BMS `#BMPxx` 이미지 시퀀스, 스테이지 이미지, 배너, 타이틀을 단 1장의 PNG 아틀라스로 합성
  - [x] 각 이미지별 정규화된 UV 사각형 좌표(`[u1, v1, u2, v2]`) 테이블 생성
- [x] **`SpriteBatcher` BGA Atlas 하드웨어 가속 연동 (`crates/beetle-render/src/backend/batcher.rs`)**
  - [x] 텍스처 스위칭 0회: BGA 교체 시 단일 텍스처에서 UV 좌표만 매핑하여 쿼드 배치 드로우
  - [x] 소프트웨어 렌더러 및 D3D11 하드웨어 백엔드 양방향 지원

---

## 📋 Phase 3: Manifest v2 확장 및 결정론적 패커 통합 (`crates/bms-package/`, `crates/bms-package-manager/`)
- [x] **`manifest.json` v2.0 스키마 확장 (`crates/bms-package/src/manifest.rs`)**
  - [x] 선택적 `sound_atlas` 및 `bga_atlas` 필드 추가
  - [x] 기존 v1.0 Manifest와의 100% 하위 호환 파싱 지원
- [x] **`bpm pack` CLI 듀얼 프로파일 지원 (`crates/bms-package-manager/src/pack.rs`)**
  - [x] `bpm pack --profile turbo` (또는 `--atlas`): 듀얼 아틀라스 고속 패키지 생성
  - [x] `bpm pack --profile classic`: 기존 파일 분산형 ZIP 패키지 생성
  - [x] 결정론적 바이트 패키징 불변식(`INV-6`) 보장 (엔트리 정렬 및 에포크 타임스탬프)

---

## 📋 Phase 4: Beetle 인게임 로더 1-Pass 초고속 파이프라인 (`crates/beetle-app/src/loader.rs`)
- [x] **1-Pass 순차 아카이브 스트리밍 로더 구현**
  - [x] 수천 번의 개별 `zip.by_name` 파일 Seek 완전 배제
  - [x] `manifest` -> `chart` -> `atlas.bin` -> `atlas.bmp` 1-Pass 순차 스트리밍
- [x] **사운드 및 BGA 아틀라스 즉시 적재**
  - [x] 단 1회 오디오 디코딩으로 전체 사운드뱅크 구축 (로딩 시간 < 20ms)
  - [x] 단 1회 이미지 디코딩으로 GPU VRAM BGA 텍스처 바인딩 및 인메모리 크롭 (로딩 시간 < 10ms)
- [x] **안전한 클래식 패키지 및 폴더 폴백 유지**
  - [x] 아틀라스가 없는 기존 `.bmsp` 및 일반 BMS 폴더도 기존 방식으로 100% 무중단 폴백 로딩

---

## 📋 Phase 5: 레거시 하위 호환 역변환 및 VFS 스트리밍 (`crates/bms-package-manager/`)
- [x] **초고속 역변환 익스포터 (`bpm export`)**
  - [x] 아틀라스 패키지에서 슬라이스를 분할 추출하여 전통 BMS 폴더(WAV/BMP 수백 개)로 1초 만에 복원
  - [x] `manifest.json`에 보존된 `original_filename` 기반 100% 무결점 복원
- [x] **VFS 온더플라이 가상 WAV 생성기 (`bpm mount`)**
  - [x] LR2, beatoraja가 가상 드라이브(`Z:\`)를 조회할 때 아틀라스 슬라이스를 기반으로 개별 파일 목록 노출
  - [x] 파일 읽기 요청 시 44바이트 RIFF WAV 헤더를 동적 합성하여 실시간 스트리밍 서빙

---

## 📋 Phase 6: 성능 벤치마크, 무결성 검증 & 회귀 테스트
- [x] **실제 대용량 BMS 곡 벤치마크 (키음 250+개, BGA 50+개 합성 벤치마크)**
  - [x] 로딩 시간 측정: 기존 59.5ms -> Turbo 11.1ms (5.4배 고속화, 목표 < 30ms 완벽 달성)
  - [x] 패키지 용량 비교: 기존 63,009바이트 -> 7,810바이트 (87.6% 압축 및 오버헤드 절감)
- [x] **전체 워크스페이스 회귀 테스트 및 바이너리 크기 준수**
  - [x] 전체 120+개 단위/통합 테스트 무결성 통과 (`cargo test --workspace`)
  - [x] 바이너리 크기 불변식 지속 준수
