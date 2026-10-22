# TASKS.md — Beetle 로드맵 및 개발 체크리스트 (Milestone 12)

이 문서는 Beetle 프로젝트의 활성 마일스톤 구현 태스크를 관리하는 로드맵 문서입니다.

> 💡 **이전 마일스톤 완료 내역 아카이브**:
> - [archive/tasks_milestone_1.md](archive/tasks_milestone_1.md): 기반 아키텍처, 오디오 엔진, 패키지 포맷 및 1차 게임 루프
> - [archive/tasks_milestone_2.md](archive/tasks_milestone_2.md): UI/UX 전면 개편, 다국어 폰트, 인게임 일시정지, 결과 보상 화면, 1:1 키 리바인딩
> - [archive/tasks_milestone_3.md](archive/tasks_milestone_3.md): 아키텍처 모듈화 & 클린 구조 리팩토링 (`beetle-render` 및 `beetle-app` 서브모듈화)
> - [archive/tasks_milestone_4.md](archive/tasks_milestone_4.md): BMS Package Delta(차분) 및 원자적 업데이트 엔진 (`.bmdp` 빌더, 패치 및 GUI 마법사)
> - [archive/tasks_milestone_5.md](archive/tasks_milestone_5.md): 디스플레이 다원화, 종횡비 보존 렌더링, 초고주사율 최적화, BGA 엔진 및 WMF 동영상 지원
> - [archive/tasks_milestone_6.md](archive/tasks_milestone_6.md): 초경량 멀티 백엔드 GPU 하드웨어 가속 렌더링 엔진, CJK 한자 폴백, 진성 GPU 배치 파이프라인
> - [archive/tasks_milestone_7.md](archive/tasks_milestone_7.md): 듀얼 아틀라스(Sound & BGA Atlas) 초고속 패키지 엔진, 무손실 FLAC 압축, WebDAV VFS 온더플라이 WAV 합성
> - [archive/tasks_milestone_8.md](archive/tasks_milestone_8.md): 원격 패키지 레지스트리 및 온라인 송 허브 (`bpm update/search/install/upgrade`, `bpm serve`, `bpm-gui` 온라인 탭)
> - [archive/tasks_milestone_9.md](archive/tasks_milestone_9.md): 순수 .bmsp 아카이브 전용 저장소 및 UI 디자인 시스템 수립
> - [archive/tasks_milestone_10.md](archive/tasks_milestone_10.md): 멀티키 모드 실동작 지원 (9K / 10K / 14K, DP 듀얼 플레이필드, 전용 키 프리셋)
> - [archive/tasks_milestone_11.md](archive/tasks_milestone_11.md): 게임 안에서 곡 넣기 (끌어다 놓기 설치, 폴더 연결, 곡 메뉴, `.bmsp` 파일 연결·단일 인스턴스, `bpm-gui`를 고급 도구로) — 남은 확인: S4 패키지 곡 삭제·버전 바꾸기

---

# 🧹 Milestone 12 전: UI/UX 다듬기

- [x] **U1** 로딩 중에 무엇을 하는지 보여 주기: 곡 로딩은 단계(채보 → 키음 n/N → BGA 이미지 n/N → BGA)와 채워지는 막대, 부팅(서재 읽기)은 전체 스캔일 때 채보 파일 n/N (`loader::progress`, `scanner::progress`)
  - 속도: 폴더 곡의 키음·BGA 이미지를 여러 스레드로 디코딩하고 폴더 목록을 한 번만 읽음. 키음 940개 곡(릴리스, 캐시 따뜻함) 로더 약 4.4초 → 1.2초. 서재 스캔도 병렬로 읽지만 836곡 기준 약 1초로 차이 없음
- [x] **U2** 폴더별로 보기: 최상위 "폴더별" — 곡 폴더를 담은 폴더(이벤트·모음 단위)마다 하나, 설치한 곡은 한데 (`folders.rs` `dir_folders`)
- [x] **U3** 한 폴더의 채보는 한 곡으로 묶기: 묶음 키를 폴더+제목에서 폴더로. 여러 곡이 섞인 폴더(제목 앞 4글자가 셋 넘게, 채보 수 절반 넘게 다름)는 예전처럼 제목으로 나눔 (`group_keys`)
- [ ] **U4** BGA가 없을 때 플레이 화면의 빈 자리 채우기
- [ ] **U5** 여백 없이 붙은 요소 찾아 고치기

# 🚀 Milestone 12: (정하는 중)

- [ ] 범위 정하기

---

## 🎨 UI 디자인 시스템
- [specs/ui_design_system.md](specs/ui_design_system.md): 타이포그래피 4단계 위계, 색상 의미 체계, 테두리 사용 정책(보더 다이어트), 리스트/모달 그룹핑 컨벤션. 화면 UI를 고칠 때 이 문서를 기준으로 삼는다.

## 🔭 향후 확장 제안 및 백로그 (Future Proposals & Backlog)
- [proposals/legacy_compatibility_vfs.md](proposals/legacy_compatibility_vfs.md): 레거시 구동기(LR2/beatoraja) 하위 호환을 위한 무설치 WebDAV VFS 마운트 및 FUSE 확장 제안서.
- [proposals/platform_expansion.md](proposals/platform_expansion.md): Linux 네이티브 데스크톱 지원, WebAssembly(WASM/Web Audio) 무설치 웹 플레이어/뷰어, 모바일/태블릿 터치 제스처 지원 제안서.
- [proposals/decoupled_bga_package_system.md](proposals/decoupled_bga_package_system.md): 오디오/차트 코어 패키지와 대용량 BGA 패키지의 완전 분리 및 온디맨드 결합 제안서.
- [proposals/gameplay_enhancement_and_display.md](proposals/gameplay_enhancement_and_display.md): 플레이 옵션(스피드, 판정 오프셋) 및 디스플레이 고도화 제안서.
- [proposals/bmz_player_feature_candidates.md](proposals/bmz_player_feature_candidates.md): bmz-player 비교 분석 및 도입 후보(파서 관용성, 차트 해시, 판정 auto-adjust, LN 모드, 볼륨 정규화, 스코어 DB 마이그레이션 등) 정리.
