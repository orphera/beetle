# TASKS.md — Beetle 로드맵 및 개발 체크리스트 (Milestone 5 Archive)

이 문서는 완료된 Beetle Milestone 5 개발 태스크 아카이브입니다.

---

# 🚀 Milestone 5: 디스플레이 다원화, 종횡비 보존 렌더링, 초고주사율 최적화 및 BGA 엔진

자세한 기술 설계 및 아키텍처는 [proposals/gameplay_enhancement_and_display.md](../proposals/gameplay_enhancement_and_display.md)를 참조합니다.

---

## 📋 Phase 1: 디스플레이 및 창 모드 시스템 (Display & Window Mode Management)
- [x] **창 모드 전환 기능 구현 (`crates/beetle-app/`)**
  - [x] `Fullscreen (독점 전체화면)`: 모니터 최고 주사율 및 독점 제어
  - [x] `Borderless Fullscreen (테두리 없는 전체화면)`: 데스크톱 해상도/주사율 유지 창 모드
  - [x] `Windowed (창모드)`: 자유로운 윈도우 크기 조절 및 위치 이동
  - [x] `Alt + Enter` 또는 옵션 모달(`Tab`)을 통한 실시간 즉시 전환
- [x] **해상도 조절 및 가상 프레임버퍼 스케일러**
  - [x] 가상 렌더링 해상도(720p, 1080p, 1440p, 4K) 선택 지원
  - [x] 창 크기 변경 시 비율 유지 레터박스 또는 정수 배율(Integer Scaling) 출력
  - [x] `config.dat`에 `display_mode`, `window_width`, `window_height`, `target_fps` 지속 저장 및 복원

---

## 📋 Phase 2: 반응형 이미지 종횡비 보존 렌더링 (Aspect-Ratio Aware Image Fitting)
- [x] **이미지 왜곡 방지 스케일러 구현 (`crates/beetle-render/src/image/`)**
  - [x] `Fill & Center Crop`: 이미지의 원래 종횡비를 엄격하게 보존하며 대상 영역을 채우고 넘치는 영역을 중앙 기준 크롭
  - [x] `Fit & Letterbox`: 원본 비율을 보존하여 전체 이미지를 표시하고 남는 여백에 다크 패딩/블러 적용
- [x] **선곡 창 및 인게임 HUD 적용**
  - [x] 선곡 창 자켓/배너 이미지 영역에서 4:3, 16:9, 1:1 이미지가 찌그러지지 않고 깔끔하게 렌더링되도록 적용
  - [x] 인게임 $320 \times 180$ 스테이지/BGA 뷰포트에 종횡비 보존 크롭 적용

---

## 📋 Phase 3: 인게임 초고주사율 프레임 페이싱 & 렌더러 극한 최적화 (High-Refresh Rate Performance)
- [x] **Windows 고정밀 프레임 타이머 연동**
  - [x] Windows 멀티미디어 타이머 `timeBeginPeriod(1)` 적용으로 1ms 이하 슬립 정밀도 확보 및 마이크로 지터 제거
  - [x] 144Hz / 240Hz / 360Hz+ 주사율 타겟 고정밀 프레임 페이서 구축
- [x] **소프트웨어 렌더러 SIMD / 청크 최적화 (`crates/beetle-render/`)**
  - [x] `draw_rect` 및 픽셀 블릿 루프 32-bit `u32` 청크/SIMD 병렬화
  - [x] 정적 배경 및 HUD 영역 Dirty Rect 캐싱을 통한 불필요한 재렌더링 절감

---

## 📋 Phase 4: BGA (BackGround Animation) 시스템 지원 (BGA & Animated Stage Rendering)
- [x] **BMS BGA 채널 파서 확장 (`crates/beetle-core/src/bms.rs`)**
  - [x] `#BMPxx` 인덱스별 이미지 시퀀스 및 비디오 파일명 매핑 파싱
  - [x] `#BGAxx` 좌표 슬라이스 및 투명색(ColorKey) 레이어 정의 파싱
  - [x] BGA 이벤트 채널(04: Base, 06: Poor, 07: Layer) 타임라인 등록
- [x] **실시간 BGA 믹서 및 애니메이션 렌더러 (`crates/beetle-render/`, `crates/beetle-app/`)**
  - [x] 곡 로딩 시 BGA 이미지 시퀀스를 사전 메모리 텍스처 풀로 적재 (`INV-3` 준수)
  - [x] `AudioClock` 시간에 동기화되어 $O(1)$로 현재 프레임 이미지를 인게임 BGA 뷰포트에 블릿
  - [x] 미스/POOR 발생 시 POOR BGA 프레임 오버레이
  - [x] `bga-enhanced` 피처: 순수 Rust PNG/JPEG 디코더 및 WMF(Windows Media Foundation) 네이티브 비디오 재생기 통합
