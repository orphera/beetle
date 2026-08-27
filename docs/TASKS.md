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

## 📋 Phase 6: 최적화 및 v1 릴리스 검증 (Completed)
- [x] **바이너리 크기 최적화 및 측정**
  - [x] Release 바이너리 크기: **~659 KB (0.63 MB)** (목표인 수 MB 이하 초과 달성)
- [x] **단위 테스트 및 품질 검증**
  - [x] 워크스페이스 28개 단위 테스트 100% 통과 (0 errors, 0 warnings)
- [x] **오디오/판정 결정론적 클럭 모델 및 락프리 구조 검증**

---

## 📋 Phase 7: 플레이 옵션(Modifiers) & 배속/판정 오프셋 시스템 (Completed)
- [x] **노트 배치 모디파이어 (Lane Modifiers) 구현 (`beetle-core`)**
  - [x] `Regular` (기본)
  - [x] `Mirror` (1~7건반 좌우 대칭 반전)
  - [x] `Random` (1~7건반 레인 무작위 셔플)
  - [x] `R-Random` (순환 회전 셔플)
  - [x] `S-Random` (노트 단위 슈퍼 랜덤)
- [x] **배속(Hi-Speed) 및 플로팅 스크롤 시스템**
  - [x] Hi-Speed 픽셀 스크롤 속도 동적 적용
  - [x] 인게임 실시간 배속 조절 단축키 (F3/F4, PageUp/PageDown) 지원
- [x] **게이지 모드 확장**
  - [x] `Easy`, `Groove(Normal)`, `Hard`, `Hazard` (1콤보 이탈 즉시 폭사) 모드 및 전용 게이지 색상/기준선
- [x] **정밀 판정 오프셋 (Calibration Offset)**
  - [x] 하드웨어/디스플레이 레이턴시 보정 (F8/F9 조절 지원)
- [x] **모디파이어 단위 테스트 (2개 테스트 통과, 총 30개 테스트 완료)**

---

## 📋 Phase 8: 선곡창 고도화 & 옵션 팝업 모달창 (Completed)
- [x] **플레이 옵션 모달 UI (`SoftwareRenderer` & `beetle-app`)**
  - [x] 선곡창에서 `Tab` / `O` 키로 옵션 패널 팝업 오버레이
  - [x] Hi-Speed, 노트 모디파이어, 게이지, 판정 오프셋, 키 레이아웃 직관적 변경
- [x] **곡 목록 정렬 (`SortMode`)**
  - [x] Title / Level / Clear Lamp / Score Rate / BPM 정렬 (`F2` 순환 토글)
- [x] **정렬 단위 테스트 (1개 테스트 통과, 총 31개 테스트 완료)**

---

## 📋 Phase 9: 인게임 UX 디테일 (FAST/SLOW & 레인커버) (Completed)
- [x] **실시간 FAST / SLOW 밀리초 표시**
  - [x] 판정선 부근에 `FAST -12ms` / `SLOW +15ms` 실시간 델타 피드백 출력
- [x] **레인 커버 (Sudden+)**
  - [x] `F10` / `F11` 키로 상단 가림막 비율 실시간 조절 및 마스킹
- [x] **페이스메이커 (Target Pacemaker)**
  - [x] 현재 노트 진행 대비 AAA 목표치와의 실시간 격차 (`PACEMAKER (AAA): +14`) 표시

---

## 📋 Phase 10: 인터랙티브 키설정 GUI & `config.dat` 영구화 (Completed)
- [x] **키 바인딩 GUI 설정 인터페이스 (`AppScreen::KeyConfig`)**
  - [x] `F12` / `C` 키로 전용 키설정 화면 진입 및 7K + 1S 레인별 키 매핑
- [x] **설정 파일 (`config.dat`) 영구 입출력**
  - [x] Hi-Speed, 노트 모디파이어, 게이지, 판정 오프셋, 레인커버, 정렬 모드, 키 프리셋 자동 저장 및 복원
- [x] **설정 직렬화 단위 테스트 (1개 테스트 통과, 총 32개 테스트 완료)**

---

## 📋 Phase 11: OGG Vorbis 키음 디코딩 & 스마트 확장자 매칭
- [ ] **`lewton` 기반 OGG Vorbis 디코더 연동 (`beetle-audio`)**
  - [ ] 순수 Rust 경량 OGG 디코더로 PCM f32 추출
- [ ] **스마트 오디오 파일 로더 (`SampleBank`)**
  - [ ] `#WAVxx file.wav` 정의 시 `.wav`, `.ogg` 자동 탐색 및 폴백
  - [ ] 파일 확장자 및 대소문자 무시 검색
- [ ] **OGG 디코딩 단위 테스트 작성**

---

## 📋 Phase 12: 오토플레이(AutoPlay) & 프랙티스/구간 연습 모드
- [ ] **오토플레이 엔진 (`beetle-core` & `beetle-app`)**
  - [ ] 오디오 타임라인 기준 PGREAT 정타이밍 자동 타건 및 키음 재생
  - [ ] 선곡창에서 `A` 키로 오토플레이 토글 및 전용 [AUTO PLAY] 인게임 배너
- [ ] **프랙티스 / 시작 마디 지정 (Practice Mode)**
  - [ ] 특정 마디(Measure)부터 즉시 시작 및 구간 반복 지원

---

## 📋 Phase 13: 리플레이 시스템(Replay) & 고스트 배틀
- [ ] **초경량 리플레이 레코더/파서 (`.rep`)**
  - [ ] 키 입력 이벤트 및 타임스탬프 플랫 파일 저장
- [ ] **리플레이 뷰어 모드**
  - [ ] 선곡창에서 이전 최고 기록 리플레이 재생
- [ ] **고스트 페이스메이커 (Ghost Pacemaker)**
  - [ ] 자신의 최고 기록 고스트와 실시간 점수/타이밍 격차 비교

---

## 📋 Phase 14: 미니멀 오디오 스펙트럼 비주얼라이저
- [ ] **실시간 오디오 버퍼 스냅샷 추출 (`beetle-audio`)**
  - [ ] 오디오 콜백 스레드에서 믹싱된 최근 PCM 데이터를 락프리로 공유
- [ ] **2D 파형 / 스펙트럼 렌더러 (`beetle-render`)**
  - [ ] 사이드 BGA 영역에 반응형 오디오 이퀄라이저 바 및 파형 렌더링


