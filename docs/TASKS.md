# TASKS.md — Beetle 로드맵 및 개발 체크리스트

이 문서는 Beetle 프로젝트의 구현 태스크를 Phase별로 정리한 로드맵입니다. 각 단계는 크레이트 구조 및 의존성 순서에 따라 설계되었습니다.

---

## 📋 Phase 0: 리포지토리 부트스트랩 및 기반 구축 (Current)
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

## 📋 Phase 6: 최적화 및 v1 릴리스 검증
- [ ] 바이너리 크기 측정 및 dead code 제거 검증 (타겟: 수 MB 수준)
- [ ] 저사양 머신 60+ FPS 소프트웨어 렌더링 프로파일링
- [ ] 판정 지연(Input-to-Audio Latency) 및 클럭 안정성 검증
