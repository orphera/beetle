# TASKS.md — Beetle 로드맵 및 개발 체크리스트 (Milestone 7)

이 문서는 Beetle 프로젝트의 활성 마일스톤 구현 태스크를 관리하는 로드맵 문서입니다.

> 💡 **이전 마일스톤 완료 내역 아카이브**:
> - [archive/tasks_milestone_1.md](archive/tasks_milestone_1.md): 기반 아키텍처, 오디오 엔진, 패키지 포맷 및 1차 게임 루프
> - [archive/tasks_milestone_2.md](archive/tasks_milestone_2.md): UI/UX 전면 개편, 다국어 폰트, 인게임 일시정지, 결과 보상 화면, 1:1 키 리바인딩
> - [archive/tasks_milestone_3.md](archive/tasks_milestone_3.md): 아키텍처 모듈화 & 클린 구조 리팩토링 (`beetle-render` 및 `beetle-app` 서브모듈화)
> - [archive/tasks_milestone_4.md](archive/tasks_milestone_4.md): BMS Package Delta(차분) 및 원자적 업데이트 엔진 (`.bmdp` 빌더, 패치 및 GUI 마법사)
> - [archive/tasks_milestone_5.md](archive/tasks_milestone_5.md): 디스플레이 다원화, 종횡비 보존 렌더링, 초고주사율 최적화, BGA 엔진 및 WMF 동영상 지원
> - [archive/tasks_milestone_6.md](archive/tasks_milestone_6.md): 초경량 멀티 백엔드 GPU 하드웨어 가속 렌더링 엔진, CJK 한자 폴백, 진성 GPU 배치 파이프라인

---

# 🚀 Milestone 7: 듀얼 아틀라스 기반 초고속 BMS 패키지 엔진 (Dual Atlas Package Engine: Sound & BGA Atlas)

자세한 기술 설계 및 아키텍처는 [proposals/dual_atlas_package.md](proposals/dual_atlas_package.md)를 참조합니다.

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
- [ ] **1-Pass 순차 아카이브 스트리밍 로더 구현**
  - [ ] 수천 번의 개별 `zip.by_name` 파일 Seek 완전 배제
  - [ ] `manifest` -> `chart` -> `atlas.flac/pcm` -> `atlas.png` 1-Pass 순차 스트리밍
- [ ] **사운드 및 BGA 아틀라스 즉시 적재**
  - [ ] 단 1회 오디오 디코딩으로 전체 사운드뱅크 구축 (로딩 시간 < 20ms)
  - [ ] 단 1회 이미지 디코딩으로 GPU VRAM BGA 텍스처 바인딩 (로딩 시간 < 10ms)
- [ ] **안전한 클래식 패키지 및 폴더 폴백 유지**
  - [ ] 아틀라스가 없는 기존 `.bmsp` 및 일반 BMS 폴더도 기존 방식으로 100% 무중단 폴백 로딩

---

## 📋 Phase 5: 레거시 하위 호환 역변환 및 VFS 스트리밍 (`crates/bms-package-manager/`)
- [ ] **초고속 역변환 익스포터 (`bpm export`)**
  - [ ] 아틀라스 패키지에서 슬라이스를 분할 추출하여 전통 BMS 폴더(WAV/BMP 수백 개)로 1초 만에 복원
  - [ ] `manifest.json`에 보존된 `original_filename` 기반 100% 무결점 복원
- [ ] **VFS 온더플라이 가상 WAV 생성기 (`bpm mount`)**
  - [ ] LR2, beatoraja가 가상 드라이브(`Z:\`)를 조회할 때 아틀라스 슬라이스를 기반으로 개별 파일 목록 노출
  - [ ] 파일 읽기 요청 시 44바이트 RIFF WAV 헤더를 동적 합성하여 실시간 스트리밍 서빙

---

## 📋 Phase 6: 성능 벤치마크, 무결성 검증 & 회귀 테스트
- [ ] **실제 대용량 BMS 곡 벤치마크 (키음 1,000+개, BGA 200+개)**
  - [ ] 로딩 시간 측정: 기존 1.5~2.5초 -> 목표 < 30ms 달성 검증
  - [ ] 패키지 용량 비교: 기존 대비 30~50% 용량 절감 검증
- [ ] **전체 워크스페이스 회귀 테스트 및 바이너리 크기 준수**
  - [ ] 전체 단위/통합 테스트 무결성 통과
  - [ ] 바이너리 크기 불변식 (< 1.2 MB) 지속 준수

---

## 🔭 향후 확장 제안 및 백로그 (Future Proposals & Backlog)
- [proposals/remote_package_registry.md](proposals/remote_package_registry.md): 원격 패키지 레지스트리, 1-클릭 다운로드/업데이트, 정적 CDN 호스팅, LAN P2P 공유 제안서.
- [proposals/legacy_compatibility_vfs.md](proposals/legacy_compatibility_vfs.md): 레거시 구동기(LR2/beatoraja) 하위 호환을 위한 무설치 WebDAV VFS 마운트 및 FUSE 확장 제안서.
- [proposals/platform_expansion.md](proposals/platform_expansion.md): Linux 네이티브 데스크톱 지원, WebAssembly(WASM/Web Audio) 무설치 웹 플레이어/뷰어, 모바일/태블릿 터치 제스처 지원 제안서.
- [proposals/lightweight_gpu_acceleration.md](proposals/lightweight_gpu_acceleration.md): 초경량 멀티 백엔드 GPU 하드웨어 가속 렌더링 엔진 제안서 (Milestone 6 Archive).
