# tasks_milestone_3.md — Milestone 3 작업 아카이브 (Clean Architecture & Modularization)

이 문서는 **Milestone 3 (아키텍처 모듈화 & 클린 구조 리팩토링)**의 전체 구현 내역 및 완료된 체크리스트를 영구 보관하는 아카이브 문서입니다.

---

# 🚀 Milestone 3: 아키텍처 모듈화 & 클린 구조 리팩토링 (Clean Architecture & Modularization) (Completed)

Milestone 3의 목표는 마일스톤 1 & 2를 거치며 급격히 거대해진 `beetle-render` (2,076줄 ➔ 248줄)와 `beetle-app` (1,607줄 ➔ 450줄)의 모놀리식 파일들을 **도메인 및 화면 단위 서브모듈로 깔끔하게 분리하고 응집도를 높여 유지보수성과 확장성을 극대화**하는 것이었습니다.

---

## 📋 Phase 1: `beetle-render` 화면별 렌더링 서브모듈 분리 (`crates/beetle-render/src/screens/`) (Completed)
- [x] **화면별 전용 렌더링 모듈 생성**
  - [x] `screens/mod.rs`: 서브모듈 재내보내기
  - [x] `screens/song_select.rs`: 선곡 캐러셀 휠, 클리어 램프 바, 상세 카드, 검색창 렌더링
  - [x] `screens/gameplay.rs`: 인게임 플레이필드, 노트, 판정선, 글로우, Danger 점멸, BGA
  - [x] `screens/result.rs`: 8단계 랭크 엠블럼, 타이밍 히스토그램, PB 델타 비교 카드
  - [x] `screens/key_config.rs`: 1:1 키 리바인딩 테이블 및 상태 프롬프트
  - [x] `screens/modals.rs`: 일시정지 모달, 옵션 모달, 로딩 스피너 화면
- [x] **`renderer.rs` 코어 다이어트 (2,076줄 ➔ 248줄로 축소 달성)**
  - [x] 순수 프레임버퍼, 뷰포트, 기하 도형 프리미티브, 스킨/히트버스트 코어만 유지
- [x] **단위 테스트 100% 통과 검증**

---

## 📋 Phase 2: `beetle-app` 백그라운드 로더 & 에셋 캡슐화 (`crates/beetle-app/src/loader.rs`) (Completed)
- [x] **비동기 로딩 및 디코딩 서브모듈 분리 (`loader.rs`)**
  - [x] `load_chart_and_audio`, `load_stage_image`, `load_preview_sample` 모듈 분리
  - [x] 백그라운드 Worker 스레드 채널 통신 캡슐화 (`spawn_background_song_loader`)
- [x] **`main.rs` 로딩 파이프라인 정리**

---

## 📋 Phase 3: `beetle-app` 화면별 입력 핸들러 및 상태 분리 (`crates/beetle-app/src/handlers/`, `state.rs`, `gameplay.rs`) (Completed)
- [x] **화면별 입력 핸들러 분리 (`handlers/`)**
  - [x] `handlers/song_select.rs`: 선곡 네비게이션, 검색 쿼리, 카테고리/정렬 토글
  - [x] `handlers/gameplay.rs`: 인게임 타건/판정, 일시정지 모달 네비게이션, 실시간 핫키
  - [x] `handlers/result.rs`: 결과 화면 키 네비게이션, 리트라이, 스크린샷 캡처
  - [x] `handlers/key_config.rs`: 1:1 키 리바인딩 인터랙션, 프리셋 토글/초기화
  - [x] `handlers/options.rs`: 옵션 모달 슬라이더/토글 네비게이션
- [x] **`AppState` 구조화 및 `state.rs` 분리**
  - [x] 전역 공통 상태와 화면별 종속 상태의 명확한 경계 수립
  - [x] `gameplay.rs`로 게임플레이 라이프사이클 분리 (`queue_start_gameplay`, `finalize_start_gameplay`, `finish_gameplay`)
- [x] **`main.rs` 경량화 (1,607줄 ➔ 450줄로 72% 축소 달성)**

---

## 📋 Phase 4: 전체 워크스페이스 회귀 검증 및 바이너리/성능 프로파일링 (Completed)
- [x] **전체 63개 단위 테스트 회귀 검증 (`cargo test --workspace` 100% Pass)**
- [x] **바이너리 크기 < 1 MB 불변식 재측정 (`beetle-app.exe: 863 KB`, `bpm-gui.exe: 860 KB`, `bpm.exe: 469 KB`)**
- [x] **오디오 락프리 & 60+ FPS 렌더링 무결성 확인**
