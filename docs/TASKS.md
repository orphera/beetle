# TASKS.md — Beetle 로드맵 및 개발 체크리스트 (Milestone 8)

이 문서는 Beetle 프로젝트의 활성 마일스톤 구현 태스크를 관리하는 로드맵 문서입니다.

> 💡 **이전 마일스톤 완료 내역 아카이브**:
> - [archive/tasks_milestone_1.md](archive/tasks_milestone_1.md): 기반 아키텍처, 오디오 엔진, 패키지 포맷 및 1차 게임 루프
> - [archive/tasks_milestone_2.md](archive/tasks_milestone_2.md): UI/UX 전면 개편, 다국어 폰트, 인게임 일시정지, 결과 보상 화면, 1:1 키 리바인딩
> - [archive/tasks_milestone_3.md](archive/tasks_milestone_3.md): 아키텍처 모듈화 & 클린 구조 리팩토링 (`beetle-render` 및 `beetle-app` 서브모듈화)
> - [archive/tasks_milestone_4.md](archive/tasks_milestone_4.md): BMS Package Delta(차분) 및 원자적 업데이트 엔진 (`.bmdp` 빌더, 패치 및 GUI 마법사)
> - [archive/tasks_milestone_5.md](archive/tasks_milestone_5.md): 디스플레이 다원화, 종횡비 보존 렌더링, 초고주사율 최적화, BGA 엔진 및 WMF 동영상 지원
> - [archive/tasks_milestone_6.md](archive/tasks_milestone_6.md): 초경량 멀티 백엔드 GPU 하드웨어 가속 렌더링 엔진, CJK 한자 폴백, 진성 GPU 배치 파이프라인
> - [archive/tasks_milestone_7.md](archive/tasks_milestone_7.md): 듀얼 아틀라스(Sound & BGA Atlas) 초고속 패키지 엔진, 무손실 FLAC 압축, WebDAV VFS 온더플라이 WAV 합성

---

# 🚀 Milestone 8: 원격 패키지 레지스트리 및 온라인 송 허브 (Remote Package Registry & Online Song Hub)

자세한 기술 설계 및 아키텍처는 [specs/remote_package_registry.md](specs/remote_package_registry.md) 및 [proposals/remote_package_registry.md](proposals/remote_package_registry.md)를 참조합니다.

---

## 📋 Phase 1: 원격 레지스트리 데이터 모델 및 경량 HTTP 클라이언트 (`crates/bms-package-manager/`)
- [x] **원격 레지스트리 인덱스 데이터 모델 및 직렬화 구현 (`crates/bms-package-manager/src/registry/remote.rs`)**
  - [x] `RemoteRegistryIndex`, `RemotePackageMetadata`, `CompanionBgaMetadata` 구조체 정의
  - [x] Serde 기반 JSON 직렬화/역직렬화 및 규격 유효성 검증 (`format_version`, `sha256` 64자 포맷)
- [x] **로컬 레지스트리 소스 설정 관리 (`crates/bms-package-manager/src/registry/sources.rs`)**
  - [x] `RegistrySource` 구조체 (`id: String`, `name: String`, `url: String`, `enabled: bool`, `priority: u32`)
  - [x] `sources.json` 파일 저장 및 로드, 기본 공식 소스(`official`) 초기화
  - [x] 소스 우선순위 기반 복수 소스 패키지 병합 로직 구축
- [x] **경량 동기식 HTTP 클라이언트 모듈 구축 (`crates/bms-package-manager/src/net/http.rs`)**
  - [x] `ureq` (with `tls`/`rustls`) 최소 의존성 추가 (Tokio/Reqwest 완전 배제, 바이너리 오버헤드 < 150 KB)
  - [x] 공격적 타임아웃(연결 3초 / 읽기 10초) 및 표준 User-Agent 헤더 설정
  - [x] 실시간 프로그레스 스트리밍 트레이트 `DownloadProgressCallback` 정의
  - [x] 다운로드 중 스트리밍 SHA-256 누적 계산 및 `index.json` 대조 검증 파이프라인

---

## 📋 Phase 2: 원격 패키지 다운로드 & 원자적 설치 파이프라인 (`crates/bms-package-manager/`)
- [x] **원격 인덱스 로컬 캐시 관리 & 오프라인 폴백 (`crates/bms-package-manager/src/net/cache.rs`)**
  - [x] `.cache/registry/<source_id>.json` 파일에 원격 인덱스 캐싱
  - [x] 오프라인 회복력(Offline Resilience): 네트워크 단절 시 에러 없이 캐시된 인덱스로 조용히 폴백
- [x] **원격 패키지 다운로더 & RAII 가드 (`crates/bms-package-manager/src/net/installer.rs`)**
  - [x] `DownloadTempFile` RAII Drop Guard: 중단/취소/Panic 시 `.tmp` 파일 즉각 자동 정리
  - [x] 스트리밍 Hard Safety Cap 검사: 메타데이터 크기 105% 또는 2 GB 초과 시 즉시 차단
  - [x] SHA-256 체크섬 불일치 시 즉각적인 임시 파일 삭제 및 무결성 에러 반환
  - [x] 다운로드 및 검증 완료된 `.bmsp`를 기존 `PackageManager::install`에 전달하여 원자적 설치 재사용
  - [x] 설치 완료 시 `registry.json` 동기화 및 `songs/` 심볼릭/하드링크 활성화
- [x] **업데이트 판정 엔진 (`crates/bms-package-manager/src/net/updater.rs`)**
  - [x] 로컬에 설치된 패키지와 원격 인덱스의 최신 버전/해시 비교
  - [x] 업그레이드 대상 패키지 목록 추출 및 일괄 업데이트 트랜잭션

---

## 📋 Phase 3: `bpm` CLI 원격 네트워크 명령어 (`crates/bms-package-manager/src/main.rs`)
- [x] **`bpm update` 명령어 구현**
  - [x] 등록된 모든 활성 소스의 `index.json`을 순차 갱신하고 로컬 캐시 동기화
  - [x] 갱신된 신규/업데이트 패키지 개수 요약 출력
- [x] **`bpm search <query>` 명령어 구현**
  - [x] 제목, 아티스트, 장르, 난이도 레벨 필터링 기반 검색 테이블 출력
  - [x] 로컬 설치 여부(`[Installed]`, `[Update Available]`, `[Available]`) 뱃지 표출
- [x] **`bpm install <id>` 명령어 구현**
  - [x] 원격 패키지 메타데이터 확인 및 터미널 다운로드 프로그레스 바 렌더링
  - [x] 다운로드, 체크섬 검증, 설치, 활성화 원스톱 실행
- [x] **`bpm upgrade` 명령어 구현**
  - [x] 업데이트 가능한 모든 패키지를 일괄 다운로드/업그레이드
- [x] **`bpm source` 관리 서브커맨드 구현**
  - [x] `bpm source list`: 소스 목록, 우선순위, 활성 상태 출력
  - [x] `bpm source add <id> <url>`: 신규 레지스트리 소스 등록
  - [x] `bpm source remove <id>`: 레지스트리 소스 제거

---

## 📋 Phase 4: `bpm serve` 로컬 LAN P2P 공유 간이 서버 (`crates/bms-package-manager/src/serve.rs`)
- [ ] **표준 라이브러리 기반 미니 정적 HTTP 서버 구현**
  - [ ] 외부 웹서버 크레이트 0개, `std::net::TcpListener` 기반 경량 HTTP/1.1 구현
  - [ ] 로컬 `packages/`를 즉시 정적 `index.json`으로 합성하여 서빙 (`GET /index.json`)
  - [ ] `.bmsp` 패키지 파일 바이트 스트리밍 서빙 (`GET /packages/<filename>.bmsp`)
- [ ] **CLI 서브커맨드 통합**
  - [ ] `bpm serve [--port 8080] [--bind 0.0.0.0]`
  - [ ] 로컬 LAN IP 자동 감지 및 접속 가이드 터미널 출력 (QR코드/콘솔 URL)

---

## 📋 Phase 5: `bpm-gui` 온라인 송 허브 (Online Song Hub) 탭 및 1-클릭 설치 UI (`crates/bpm-gui/`)
- [ ] **상단 탭 네비게이션 UI 구축 (`crates/bpm-gui/src/ui.rs`)**
  - [ ] `[Installed Library]` 탭과 `[Online Song Hub]` 탭 전환 UI 제공
- [ ] **온라인 곡 탐색 카탈로그 뷰 & 가상 스크롤 (Viewport Culling)**
  - [ ] 화면 뷰포트 기반 가상 스크롤링: 현재 보이는 6~10개 카드만 슬라이싱 렌더링하여 60 FPS 불변식(`INV-5`) 유지
  - [ ] 원격 캐시 인덱스 목록 렌더링 (곡명, 아티스트, 장르, 난이도 레벨, 용량)
  - [ ] 검색창 필터링 (제목/아티스트 실시간 검색) 및 레벨 필터 버튼
- [ ] **논블로킹 백그라운드 다운로드 & 1-클릭 설치**
  - [ ] `[Install]` 클릭 시 백그라운드 Worker 스레드로 다운로드 위임 (`INV-5` 준수)
  - [ ] 다운로드 프로그레스 리드로우 스로틀링: 최대 30 FPS 주기 또는 1% 이상 변화 시에만 렌더 이벤트 발생
  - [ ] 카드별 다운로드 프로그레스 바 및 퍼센트/다운로드 속도 실시간 렌더링
  - [ ] 다운로드 완료 시 자동 설치 및 `[Installed]` 상태 갱신
- [ ] **업데이트 알림 및 원클릭 일괄 업그레이드**
  - [ ] 업데이트 가능한 곡에 `[Update Available]` 강조 뱃지 및 원클릭 업그레이드 버튼

---

## 📋 Phase 6: E2E 통합 테스트, 네트워크 오류 복원력 및 바이너리 크기 검증
- [ ] **로컬 Mock HTTP 서버 기반 통합 테스트 (`crates/bms-package-manager/tests/`)**
  - [ ] 원격 인덱스 파싱 및 다중 소스 우선순위 병합 테스트
  - [ ] 패키지 스트리밍 다운로드 및 SHA-256 무결성 검증 테스트
  - [ ] 다운로드 도중 연결 끊김 / 해시 불일치 시 롤백 및 임시 파일 정리 테스트
  - [ ] `bpm serve`와 `bpm install` 간의 로컬 루프백 P2P 다운로드 E2E 테스트
- [ ] **바이너리 크기 및 아키텍처 불변식 검증**
  - [ ] `cargo check --workspace`, `cargo test --workspace` 무결성 통과
  - [ ] 릴리스 바이너리 크기 측정 (`AGENTS.md` < 1.2 MB 목표 유지)

---

## 🔭 향후 확장 제안 및 백로그 (Future Proposals & Backlog)
- [proposals/legacy_compatibility_vfs.md](proposals/legacy_compatibility_vfs.md): 레거시 구동기(LR2/beatoraja) 하위 호환을 위한 무설치 WebDAV VFS 마운트 및 FUSE 확장 제안서.
- [proposals/platform_expansion.md](proposals/platform_expansion.md): Linux 네이티브 데스크톱 지원, WebAssembly(WASM/Web Audio) 무설치 웹 플레이어/뷰어, 모바일/태블릿 터치 제스처 지원 제안서.
- [proposals/decoupled_bga_package_system.md](proposals/decoupled_bga_package_system.md): 오디오/차트 코어 패키지와 대용량 BGA 패키지의 완전 분리 및 온디맨드 결합 제안서.
- [proposals/gameplay_enhancement_and_display.md](proposals/gameplay_enhancement_and_display.md): 플레이 옵션(스피드, 판정 오프셋) 및 디스플레이 고도화 제안서.
