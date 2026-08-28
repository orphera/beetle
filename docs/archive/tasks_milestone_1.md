# tasks_milestone_1.md — Beetle 마일스톤 1 아카이브 (Phases 0 ~ 23)

이 문서는 Beetle 프로젝트의 첫 번째 마일스톤(기반 엔진, 오디오/렌더링 코어, 패키지 포맷 및 기본 플레이어/매니저 구축)에서 완료된 구현 태스크 아카이브입니다.

---

## 📋 Phase 0: 리포지토리 부트스트랩 및 기반 구축 (Completed)
- [x] Cargo Workspace 초기화 및 릴리스 크기 최적화 프로필 설정
- [x] 4개 크레이트 분할 (`beetle-core`, `beetle-audio`, `beetle-render`, `beetle-app`)
- [x] 크레이트별 기본 모듈 스켈레톤 및 인터페이스 정의
- [x] `beetle-app` 빈 창 (`winit` + `softbuffer`) 실행 검증
- [x] 문서화 (`README.md`, `AGENTS.md`, `docs/TASKS.md`, `docs/DECISIONS.md`, `.gitignore`)

---

## 📋 Phase 1: `beetle-core` — 채보 파서 및 타이밍/판정 모델 (Completed)
- [x] **BMS / BME / BML 텍스트 파서 구현**
  - [x] `#HEADER` 태그 파싱 (`#TITLE`, `#ARTIST`, `#BPM`, `#TOTAL`, `#WAVxx`, `#BMPxx`, `#PLAYER`)
  - [x] `#MEASURE` 데이터 채널 파싱 (01: BGM, 02: 마디 길이 배율, 03/08: BPM 변경, 09: STOP, 11~19: 1P 단노트, 51~59: 1P 롱노트)
  - [x] Base36 (`01`~`ZZ`) 식별자 인코딩/디코딩 유틸리티
  - [x] LNTYPE 1 및 #LNOBJ 롱노트 처리
- [x] **타이밍 모델 (`TimingModel`) 완성**
  - [x] 고정/가변 BPM 타임라인 계산
  - [x] `#STOP` 정지 시간 계산
  - [x] 마디/박자(Measure/Fraction) ↔ 절대 시간(Seconds/Samples) 양방향 정밀 변환
- [x] **판정 엔진 (`JudgeEngine`) 구현**
  - [x] 판정 윈도우 (PGREAT / GREAT / GOOD / BAD / POOR / MISS)
  - [x] 단노트 및 롱노트(Hold / Release) 판정 로직
  - [x] 스코어(EX-Score, Rate, Combo) 및 게이지(Groove, Hard) 시뮬레이션
- [x] **파서, 타이밍, 판정 단위 테스트 작성 (13개 테스트 통과)**

---

## 📋 Phase 2: `beetle-audio` — 경량 믹서 및 오디오 클럭 (Completed)
- [x] **WAV / PCM 사전 디코더 (`SampleBank`) 구현**
  - [x] `hound` 기반 8/16/24/32비트 WAV 로더 및 Stereo f32 정규화
  - [x] `#WAVxx` 채보 오디오 사전 로드 (`load_chart_soundbank`)
- [x] **락프리 믹서 (`Mixer`) 구현**
  - [x] 고정 크기 발음 풀 (`[ActiveVoice; 128]`) 관리 및 Voice Stealing
  - [x] 선형 보간 믹싱 및 패닝/볼륨 감쇠 연산
  - [x] `rtrb` 링버퍼 커맨드 소비 (Zero-Allocation 보장)
- [x] **마스터 오디오 클럭 (`AudioClock`) 정밀화**
  - [x] `AtomicU64` 기반 샘플 누적 및 레이턴시 오프셋 보정
- [x] **오디오 엔진 통합 및 단위 테스트 (5개 테스트 통과)**

---

## 📋 Phase 3: `beetle-render` — 소프트웨어 2D 렌더러 & 폰트 (Completed)
- [x] **임베디드 비트맵 폰트 (`BitmapFont`) 작성**
  - [x] 5x7 ASCII 픽셀 아틀라스 내장 (~475B ROM)
  - [x] 스케일링/색상 텍스트 및 중앙 정렬 렌더링
- [x] **스킨 레이아웃 (`SkinConfig`) 구성**
  - [x] 7키 + 1스크래치 레인 좌표 및 너비 계산
  - [x] 판정선, 레인 구분선, 키빔(Key Beam) 색상 설정
- [x] **노트 렌더링 파이프라인 (`SoftwareRenderer`)**
  - [x] `AudioClock` 기반 가시 노트 쿼리 및 Y좌표 계산
  - [x] 단노트 및 롱노트 바디/헤드/테일 렌더링
  - [x] 판정 애니메이션 (Combo 카운터, Judge 글자 팝업)
  - [x] 그루브 / 하드 게이지 바 및 HUD 정보 렌더링
- [x] **렌더러 단위 테스트 (4개 테스트 통과)**

---

## 📋 Phase 4: `beetle-app` — 통합 게임플레이 루프 (Completed)
- [x] **입력 시스템 (`InputConfig`) 구현**
  - [x] 7K + 1S 기본 키매핑 프리셋 (HomeRow & ArcadeZx 런타임 F1/Tab 전환 지원)
  - [x] 커스텀 키 바인딩 확장 지원
  - [x] 입력 타임스탬프와 `AudioClock` 간의 판정 큐잉
  - [x] 키음 트리거 락프리 큐 전송
- [x] **인게임 상태 머신 & BGM 스케줄러**
  - [x] BMS 로딩 및 내장 데모 곡/신디사이저 사운드뱅크 지원
  - [x] 자동 BGM 재생 스케줄러 (타임라인 기반 BGM 트리거링)
- [x] **소프트버퍼 화면 출력 연동**
  - [x] `tiny-skia` 픽셀 버퍼 → `softbuffer` 프레임버퍼 다이렉트 전송
- [x] **앱 입력 단위 테스트 (2개 테스트 통과)**

---

## 📋 Phase 5: 곡 라이브러리 & 로컬 스코어 시스템 (Completed)
- [x] **곡 폴더 스캐너 & 메타데이터 캐시**
  - [x] 지정된 디렉토리 내 `.bms`/`.bme`/`.bml` 재귀 검색
  - [x] `SongMetadata` FNV-1a 해싱 및 `songs.cache` 플랫 텍스트 캐시 생성
- [x] **미니멀 선곡 화면**
  - [x] 상/하(J/K) 곡 탐색, 상세 메타데이터 및 최고 기록 패널 렌더링
  - [x] Enter/Space 즉시 플레이 및 F5 재스캔
- [x] **로컬 플랫 파일 스코어 저장**
  - [x] `ScoreStore` 및 `scores.dat` 기반 최고 기록, EX-Score, 정확도, 클리어 램프 영구 저장
- [x] **라이브러리/스코어 단위 테스트 (4개 테스트 통과)**

---

## 📋 Phase 6: 최적화 및 v1 릴리스 검증 (Completed)
- [x] **바이너리 크기 최적화 및 측정**
  - [x] Release 바이너리 크기: **~659 KB (0.63 MB)** (목표인 수 MB 이하 초과 달성)
- [x] **단위 테스트 및 품질 검증**
  - [x] 워크스페이스 단위 테스트 100% 통과 (0 errors, 0 warnings)
- [x] **오디오/판정 결정론적 클럭 모델 및 락프리 구조 검증**

---

## 📋 Phase 7: 플레이 옵션(Modifiers) & 배속/판정 오프셋 시스템 (Completed)
- [x] **노트 배치 모디파이어 (Lane Modifiers) 구현 (`beetle-core`)**
  - [x] `Regular`, `Mirror`, `Random`, `R-Random`, `S-Random`
- [x] **배속(Hi-Speed) 및 플로팅 스크롤 시스템**
  - [x] Hi-Speed 픽셀 스크롤 속도 동적 적용 및 인게임 조절 지원
- [x] **게이지 모드 확장**
  - [x] `Easy`, `Groove`, `Hard`, `Hazard` 모드 및 전용 색상/기준선
- [x] **정밀 판정 오프셋 (Calibration Offset)**
  - [x] 하드웨어/디스플레이 레이턴시 보정 지원
- [x] **모디파이어 단위 테스트 (2개 테스트 통과)**

---

## 📋 Phase 8: 선곡창 고도화 & 옵션 팝업 모달창 (Completed)
- [x] **플레이 옵션 모달 UI (`SoftwareRenderer` & `beetle-app`)**
  - [x] 선곡창에서 `Tab` / `O` 키로 옵션 패널 팝업 오버레이
- [x] **곡 목록 정렬 (`SortMode`)**
  - [x] Title / Level / Clear Lamp / Score Rate / BPM 정렬 (`F2` 순환 토글)
- [x] **정렬 단위 테스트 (1개 테스트 통과)**

---

## 📋 Phase 9: 인게임 UX 디테일 (FAST/SLOW & 레인커버) (Completed)
- [x] **실시간 FAST / SLOW 밀리초 표시**
- [x] **레인 커버 (Sudden+)**
- [x] **페이스메이커 (Target Pacemaker AAA)**

---

## 📋 Phase 10: 인터랙티브 키설정 GUI & `config.dat` 영구화 (Completed)
- [x] **키 바인딩 GUI 설정 인터페이스 (`AppScreen::KeyConfig`)**
- [x] **설정 파일 (`config.dat`) 영구 입출력 및 복원**
- [x] **설정 직렬화 단위 테스트 (1개 테스트 통과)**

---

## 📋 Phase 11: OGG Vorbis 키음 디코딩 & 스마트 확장자 매칭 (Completed)
- [x] **`lewton` 기반 OGG Vorbis 디코더 연동 (`beetle-audio`)**
- [x] **스마트 오디오 파일 로더 (`SampleBank`)**

---

## 📋 Phase 12: 오토플레이(AutoPlay) & 프랙티스/구간 연습 모드 (Completed)
- [x] **오토플레이 엔진 (`beetle-core` & `beetle-app`)**
- [x] **프랙티스 / 시작 마디 지정 (Practice Mode)**
- [x] **오토플레이 단위 테스트 (1개 테스트 통과)**

---

## 📋 Phase 13: 리플레이 시스템(Replay) & 고스트 배틀 (Completed)
- [x] **초경량 리플레이 레코더/파서 (`ReplayData` & `.rep`)**
- [x] **리플레이 뷰어 모드 (`R` 키)**
- [x] **리플레이 직렬화 단위 테스트 (1개 테스트 통과)**

---

## 📋 Phase 14: 미니멀 오디오 스펙트럼 비주얼라이저 (Completed)
- [x] **실시간 락프리 16밴드 오디오 스냅샷 (`Mixer` & `AudioEngine`)**
- [x] **2D 실시간 스펙트럼 비주얼라이저 (`SoftwareRenderer`)**

---

## 📋 Phase 15: 정적 BGA 및 타이틀 이미지 지원 (`#STAGEFILE` / `#BMP`) (Completed)
- [x] **순수 Rust 경량 BMP 디코더 구현 (`beetle-render::ImageBuffer`)**
- [x] **선곡창 STAGEFILE 썸네일 표출 및 인게임 BGA 액자 렌더링**

---

## 📋 Phase 16: 선곡창 곡 미리듣기 (Preview Audio Loop) (Completed)
- [x] **선곡창 프리뷰 오디오 루프 시스템 (`beetle-app`)**
- [x] **안전한 오디오 리소스 전환**

---

## 📋 Phase 17: 인게임 타건 파티클 & 콤보 펄스 애니메이션 (Completed)
- [x] **판정선 히트 파티클 버스트 시스템 (`HitBurst` in `SoftwareRenderer`)**
- [x] **콤보 카운터 탄성 바운스(Scale Pulse)**
- [x] **PGREAT 네온 레인 플래시 효과**

---

## 📋 Phase 18: 5키 / 9키 / 14키(DP) 다중 모드 지원 (Completed)
- [x] **BMS 채보 모드 감지 (`PlayMode`: 5K, 7K, 9K, 14K)**
- [x] **가변 플레이필드 레이아웃 및 렌더러 분기 (`SkinConfig::set_play_mode`)**

---

## 📋 Phase 19: `bms-package` 패키지 포맷 및 라이브러리 (`.bmsp`) (Completed)
- [x] **패키지 포맷 및 Manifest 모델 정의 (`Manifest`)**
- [x] **보안 경로 검증 시스템 (`validate_entry_path`)**
- [x] **결정론적 패키지 생성기 (`PackageBuilder`)**
- [x] **패키지 리더 및 스트리밍 접근 (`Package`)**

---

## 📋 Phase 20: BMS 폴더 패킹 & 임포트 엔진 (`bms-package-manager`) (Completed)
- [x] **BMS 폴더 자동 분석 및 패킹 모듈 (`pack_folder` / `analyze_bms_folder`)**
- [x] **원클릭 폴더 임포트 (`import_folder`)**
- [x] **CLI 서브커맨드 구현 (`bpm pack`, `bpm import`)**

---

## 📋 Phase 21: 독립형 경량 패키지 매니저 GUI (`bpm-gui`) (Completed)
- [x] **`crates/bpm-gui` 초경량 소프트웨어 렌더링 데스크톱 UI 구축**
- [x] **패키지 목록 탐색 및 실시간 검색 필터**
- [x] **패키지 상세 메타데이터 & BGA 아트워크 미리보기**
- [x] **다중 버전 간 활성 버전 전환 및 삭제**
- [x] **로컬 폴더/BMSP 임포트 & 패키지 내보내기 모달**

---

## 📋 Phase 22: `bpm-gui` 비동기 백그라운드 워커 & 로딩 스피너 UI (Completed)
- [x] **백그라운드 비동기 작업 큐 및 채널 (`mpsc`) 파이프라인**
- [x] **실시간 로딩 스피너 및 프로그레스 렌더링**
- [x] **작업 완료/실패 알림 및 자동 레지스트리 갱신**

---

## 📋 Phase 23: `beetle-app` 인게임 로딩 화면 (`AppScreen::Loading`) & 비동기 사운드뱅크 적재 (Completed)
- [x] **`AppScreen::Loading` 전용 로딩 화면 구현**
- [x] **백그라운드 키음 디코딩 & 오디오 엔진 준비 파이프라인**
- [x] **로딩 중 `[Esc]` 키로 선곡 화면 안전 취소 복귀 지원**
