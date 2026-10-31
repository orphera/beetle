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

- [x] **B10** 기획 일관성 점검: 화면·모드·옵션·용어·동작 규칙을 [게임 기획 기준 및 일관성 점검](plans/2026-10-31-game-design-consistency.md)에 모으고, 문서 불일치와 후속 결정을 정리
- [x] **U1** 로딩 중에 무엇을 하는지 보여 주기: 곡 로딩은 단계(채보 → 키음 n/N → BGA 이미지 n/N → BGA)와 채워지는 막대, 부팅(서재 읽기)은 전체 스캔일 때 채보 파일 n/N (`loader::progress`, `scanner::progress`)
  - 속도: 폴더 곡의 키음·BGA 이미지를 여러 스레드로 디코딩하고 폴더 목록을 한 번만 읽음. 키음 940개 곡(릴리스, 캐시 따뜻함) 로더 약 4.4초 → 1.2초. 서재 스캔의 채보 파일도 병렬로 읽음(전체 스캔 시간은 U1b 참고)
- [x] **U1b** 곡 목록 읽기(부팅) 단계와 진행 막대: 기록 → 곡 목록 캐시 → 채보 찾기(n개) → 채보 파일 n/N → 설치한 곡 n/N → 사본 고르기 → 난이도표. 단계마다 걸린 시간을 표준 출력에 `[library]`로 남김 (`boot_progress.rs`)
  - 전체 스캔이 약 140초 걸리던 원인은 같은 채보가 폴더와 패키지에 함께 있을 때(601묶음) 키음마다 디스크를 여러 번 보던 사본 고르기(약 130초). 폴더 파일 목록을 한 번만 읽고, 패키지는 항목 색인(`bms_package::EntryIndex`, `find_entry_path`와 같은 규칙 — 테스트로 고정)으로 답하고, 패키지 읽기와 사본 고르기를 여러 스레드로. 전체 스캔 약 9초, 결과 `songs.cache`는 바이트 단위로 같음. 캐시가 있으면 실행부터 선곡까지 약 1.7초
- [x] **U6** 난이도 이름을 채보의 `#DIFFICULTY`로: BEGINNER / NORMAL / HYPER / ANOTHER / INSANE, 없으면 "LV"(색은 레벨 구간). 전에는 레벨 숫자로 추측해 레벨 13 이상이 모두 OVERJOY였음. 곡 캐시 V8 (`theme::level_tier`)
- [x] **U7** 플레이 옵션 "속도 기준": 노트 속도(그린 넘버)를 맞출 BPM을 시작 / 최저 / 최고 / 주 BPM(가장 오래 유지되는 BPM) 중에서. 마지막 노트까지 실제로 유지된 BPM만 셈 (`TimingModel::bpm_stats`, `speed_anchor=` 설정)
- [x] **U8** "그린 넘버"를 화면에서는 "노트 속도"로(값은 ms 그대로, 설명에 그린 넘버라고도 한다고 적음). 선곡 칩 "G 480" → "속도 480"
- [x] **U2** 폴더별로 보기: 최상위 "폴더별" — 곡 폴더를 담은 폴더(이벤트·모음 단위)마다 하나, 설치한 곡은 한데 (`folders.rs` `dir_folders`)
- [x] **U3** 한 폴더의 채보는 한 곡으로 묶기: 묶음 키를 폴더+제목에서 폴더로. 여러 곡이 섞인 폴더(제목 앞 4글자가 셋 넘게, 채보 수 절반 넘게 다름)는 예전처럼 제목으로 나눔 (`group_keys`)
- [x] **U4** BGA가 없을 때 플레이 화면의 빈 자리 채우기: 채보에 BGA 이미지·영상이 없으면 곡 정보 아래에 스테이지 이미지 카드(없으면 레벨 색 카드)와 NOTES·GAUGE·JUDGE·TOTAL. 점수 열의 타임라인 아래에는 늘 MAX COMBO·FAST·SLOW와 타이밍 분포 막대 (`play.rs` `jacket_card`, `timing_stats`)
- [x] **U5** 여백 없이 붙은 요소 찾아 고치기: 플레이 곡 정보의 제목이 레벨 칩·자동 플레이 배지와 겹치던 것, EX SCORE 캡션과 점수 숫자, 정확도와 막대, 선곡 필터의 `~`가 양쪽 레벨 칸에 붙던 것, 결과 그래프의 "실패 0%"가 점선에 붙던 것. 타이틀·설정·키 설정·로딩은 캡처로 확인, 고칠 것 없음

# 🚀 Milestone 12: 게임 안에서 없는 곡 받기 (In-Game Missing Chart Fetch)

설계와 확정된 결정은 [plans/2026-10-22-missing-chart-fetch-in-app.md](plans/2026-10-22-missing-chart-fetch-in-app.md)를 참조합니다. 받기 규칙(직접 링크·IR 페이지·해시 확인·`songs/<표>/<#>` 보관)은 [plans/2026-10-09-missing-chart-download-design.md](plans/2026-10-09-missing-chart-download-design.md)를 그대로 따릅니다.

- [ ] **N1** 받기 로직(직접 링크, IR 페이지 링크, 해시 확인, 보관)을 `bms-package-manager`의 한 진입점으로 모으고 보고 구조체를 돌려주게 하기. `bpm`·`bpm-gui`도 같은 함수를 부름 ← 다음
- [ ] **N2** 선곡 필터 줄에 "없는 곡 포함": 켜면 난이도표 폴더에 가지지 않은 곡이 흐린 행으로 섞여 보임("없음" / "본체 필요")
- [ ] **N3** 없는 곡 행의 곡 메뉴: "받기"(받기 전에 링크 목록을 보여 주고 확인, 워커에서 받기, 진행 토스트, 해시 확인, 다시 읽고 커서 이동), "본체 페이지 열기"(브라우저)
- [ ] **N4** "본체 필요" 곡: 본체 폴더를 골라 차분을 그 폴더에 넣기(`--into`와 같은 규칙)
- [ ] **N5** 실행 파일 크기 증가(ureq + rustls) 측정과 기록

---

# 📋 백로그

마일스톤에 아직 넣지 않은 일입니다. 시작할 때 위 마일스톤 형식으로 옮기고 여기서 지웁니다.

### 순서 (선행 → 후행)

번호는 바꾸지 않고 이 표로 순서를 정합니다. 같은 단계 안은 서로 독립이라 아무거나 먼저 해도 됩니다.

| 단계 | 항목 | 선행 |
|---|---|---|
| 1 | B1 BGA 프레임 드랍, B2 부분 다시 읽기, B3 DVD 로딩 | — |
| 1 | B11 오디오 장치 선택, B13 컨트롤러, B18 스크롤바 | — |
| 1 | B15 libarchive 검토 | — |
| 2 | B9 최상위 모드 선택, B8 키 모드 표기 | B10 |
| 2 | B14 저장 방식 | B10 |
| 2 | B6 기어 리디자인 | B10 |
| 2 | B12 ASIO | B11 (장치 선택 위에 얹음) |
| 2 | B16 한 곡씩 풀어 넣기 | B15 (압축 해제 방식이 정해진 뒤) |
| 3 | B5 BPM 변화 레인 표시, B7 옵션 미리보기 | B6 (새 기어 위에 그림) |
| 3 | B17 같은 곡 묶기 | B10, B14 (묶음을 저장할 형식) |
| 4 | B4 전반 최적화 | B1, B2, B3 (구체 항목 뒤 남은 것) |

### 성능
- [ ] **B1** BGA가 있으면 플레이 중 프레임 드랍이 심함: 원인 측정(디코드·업로드·그리기 중 어디인지)부터. 영상 디코드를 워커 스레드로, 텍스처 업로드는 바뀐 프레임만 (`beetle-render` BGA 플레이어, `mpeg-video`)
- [ ] **B2** 곡 하나가 바뀌었는데 곡 목록 전체를 다시 읽는 것 같음: 언제 다시 읽는지 찾고, 바뀐 곡만 갱신
- [ ] **B3** 실제 IIDX CS DVD(광학 드라이브)에서 로딩이 오래 걸림: 유력한 원인은 스프라이트(로딩 카드 등)를 하나씩 따로 읽는 것 — 광학 드라이브는 탐색이 비싸서 작은 읽기가 많으면 느림. 필요한 범위를 섹터 순서대로 한 번에 묶어 읽고 메모리에 캐시하기. 먼저 읽기 횟수·탐색 위치를 기록해 확인 (`cs-disc`)
- [ ] **B4** 전반적인 최적화: 프로파일 한 번 돌려 큰 것부터 (구체 항목은 측정 후 쪼갬)

### 디스크
- [ ] **B19** pop'n 8~10 BGM: 8은 곡과 다른 BGM이 나옴(사용자 청취, 게임 코드 조사 중: 9.2절 '8의 조사'). 9·10은 스트림 표의 리드로 시작 시각을 고침(2026-10-30, 10은 청취 확인, 9는 확인 전). 지금까지의 조사와 다음에 볼 것은 [specs/popn_ps2_disc.md](specs/popn_ps2_disc.md) 9.2절. 확인용 `song_dump` 예제와 `BEETLE_DEV_MIX`

### 플레이·화면
- [ ] **B5** BPM이 바뀌는 지점을 레인에 표시
- [ ] **B6** 기어(레인 프레임·판정선 주변) 리디자인
- [ ] **B7** 플레이 옵션(TAB) 패널에서 바꾼 값을 미리보기로 보여 주기 (노트 속도·레인 커버 등)
- [ ] **B8** 사용자 화면의 키 모드 표기: 5K→5+1K, 7K→7+1K, 10K→10+2K, 14K→14+2K. 4K/6K/8K/9K는 별도 차트 모드명 유지. 선곡·키 설정·플레이·도움말에 함께 적용하고 설정 파일 식별자는 유지
- [ ] **B9** 플레이 방식과 콘텐츠 출처를 분리해 진입 흐름 정리: 프리 플레이 / 코스(단위인정 포함)를 고르고, 서재 / CS 디스크는 콘텐츠 출처로 선택. 단위인정은 별도 실행 모드가 아니라 제약이 있는 코스. 선곡·타이틀 진입 UI 설계 및 구현

### 오디오·입력
- [ ] **B11** 오디오 출력 장치 선택 (설정)
- [ ] **B12** ASIO 지원 (cpal `asio` 기능은 ASIO SDK가 필요 — 빌드·배포 방법부터 검토)
- [ ] **B13** 컨트롤러 지원 (게임패드/전용 컨트롤러, 키 설정에서 바인딩)

### 저장·패키지
- [ ] **B14** 저장 방식 검토 (B10 기획 정리 뒤에): 평문 저장(`scores.dat`, `courses.dat`, `library.dat`, `songs.cache` 등)은 깨지기 쉽고 오히려 느림. 기획에서 필요한 질의(기록 통계 등)가 정해지면 고름. 후보: SQLite(질의 자유, 크기 큼, 지금 의존성 정책상 금지라 도입 시 ADR 필요) / redb(순수 Rust 키-값, ACID) / 데이터별로 나누기 — 기록은 추가 전용 로그+스냅샷, 곡 캐시는 이진 형식+원자적 쓰기, 설정은 텍스트+원자적 쓰기. 첫 단계는 지금 파일들의 크기·쓰기 방식·깨진 사례 조사
- [ ] **B15** libarchive 도입 검토: 지원 압축 형식(rar/7z 등) 대비 바이너리 크기·정적 링크 가능 여부(원칙 1·2)

### BPM (bms-package-manager / bpm-gui)
- [ ] **B16** 설치할 때 공간을 너무 많이 씀: 전부 풀고 전부 넣지 말고 한 곡 풀고 → 넣고 → 지우고를 반복
- [ ] **B17** 같은 곡을 같은 곡으로 묶기. 단, 같은 곡이라도 BMS 파일 세트(차분·키음 구성)는 다를 수 있으니 묶음 안에서 세트를 구분
- [ ] **B18** `bpm-gui`에서 스크롤바를 잡고 끌어서 스크롤

---

## 🎨 UI 디자인 시스템
- [specs/ui_design_system.md](specs/ui_design_system.md): 타이포그래피 4단계 위계, 색상 의미 체계, 테두리 사용 정책(보더 다이어트), 리스트/모달 그룹핑 컨벤션. 화면 UI를 고칠 때 이 문서를 기준으로 삼는다.

## 🔭 향후 확장 제안 및 백로그 (Future Proposals & Backlog)
- [proposals/legacy_compatibility_vfs.md](proposals/legacy_compatibility_vfs.md): 레거시 구동기(LR2/beatoraja) 하위 호환을 위한 무설치 WebDAV VFS 마운트 및 FUSE 확장 제안서.
- [proposals/platform_expansion.md](proposals/platform_expansion.md): Linux 네이티브 데스크톱 지원, WebAssembly(WASM/Web Audio) 무설치 웹 플레이어/뷰어, 모바일/태블릿 터치 제스처 지원 제안서.
- [proposals/decoupled_bga_package_system.md](proposals/decoupled_bga_package_system.md): 오디오/차트 코어 패키지와 대용량 BGA 패키지의 완전 분리 및 온디맨드 결합 제안서.
- [proposals/gameplay_enhancement_and_display.md](proposals/gameplay_enhancement_and_display.md): 플레이 옵션(스피드, 판정 오프셋) 및 디스플레이 고도화 제안서.
- [proposals/bmz_player_feature_candidates.md](proposals/bmz_player_feature_candidates.md): bmz-player 비교 분석 및 도입 후보(파서 관용성, 차트 해시, 판정 auto-adjust, LN 모드, 볼륨 정규화, 스코어 DB 마이그레이션 등) 정리.
