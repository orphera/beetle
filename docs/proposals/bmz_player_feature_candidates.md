# bmz-player 비교 분석 및 기능 후보 제안서 (Feature Candidates from bmz-player)

본 문서는 [bmz-player](https://github.com/hyrorre/bmz-player)(Rust 기반 차세대 BMS 플레이어)를 참고하여, Beetle에 도입할 만한 기능 후보를 정리한 **백로그 성격의 제안서**입니다. 아직 구현이 확정된 항목은 없으며, 착수 시 별도의 `docs/plans/` 문서로 구체화합니다.

> 작성 근거: bmz-player의 공개 README(2026-10-07 시점)만 확인했고 소스 코드는 분석하지 않았습니다. 구현 방식에 대한 서술은 추정이 아닌 README에 명시된 기능 설명에 한정됩니다.

---

## 1. 두 프로젝트의 설계 방향 비교

| | **Beetle** | **bmz-player** |
| :--- | :--- | :--- |
| 최우선 가치 | 바이너리 크기(< 1 MB), 단일 정적 바이너리 | 기능 완성도, beetle 계열 기존 생태계와의 호환 |
| 렌더링 | Direct3D 11 직접 FFI (Windows 전용, WARP 폴백) | wgpu (DX12 / Metal / Vulkan) |
| 오디오 | cpal + 자체 락프리 믹서, 사전 PCM 디코딩 | FFmpeg 기반 |
| 스킨 / UI | 코드로 생성하는 기본 스킨, 임베디드 서브셋 폰트 | beatoraja 호환 Lua/JSON 스킨 엔진 |
| 곡 배포 | 자체 `.bmsp` 패키지 + `bpm` | 곡 루트 직접 스캔 (ZIP/RAR/7z 포함) |
| 부가 기능 | 패키지 레지스트리, 델타 업데이트 | 스코어 DB 마이그레이션, 웹 IR, OBS, Discord RPC |
| 플랫폼 | Windows 전용 (ADR-026) | Windows / macOS / Linux |

### 1.1 Beetle 정책과 충돌하여 채택하지 않는 항목

- **wgpu**: 금지 라이브러리 (AGENTS.md §3, ADR-026).
- **FFmpeg 의존**: "외부 런타임 없는 단일 정적 바이너리" 원칙(Core Philosophy 2)과 충돌.
- **beatoraja Lua/JSON 스킨 엔진**: 바이너리 크기 및 "코드로 생성하는 기본 스킨" 방침과 충돌.
- **Discord Rich Presence / OBS WebSocket / 웹 IR(Nuxt)**: 의존성·크기 부담 대비 우선순위 낮음.
- **ZIP/RAR/7z 곡 루트 직접 스캔**: `.bmsp` 전용 저장소 방침(Milestone 9)과 불일치. 다만 일반 BMS 폴더/ZIP을 `.bmsp`로 변환하는 import 경로는 별도 검토 가치가 있음(§3 참고).

---

## 2. 기능 후보

Beetle의 현재 상태(2026-10-07 코드 기준)와 대조한 결과입니다. 이미 구현된 것: FAST/SLOW 카운트, `judge_offset_ms`, 연습 모드(마디 점프), 리플레이, 랜덤 옵션(Mirror/RRandom 등), `#RANK` 기반 판정창, 게이지 이득 설정.

### 2.1 우선 권장 — 크기 영향 거의 없음, 코어의 순수 로직

| # | 후보 | 내용 | 비용 / 리스크 | 관련 모듈 |
| :--- | :--- | :--- | :--- | :--- |
| C1 | **파서 관용성** | `#00111 0101`처럼 콜론 대신 공백·탭으로 구분된 데이터 라인 허용. 측정 길이(`#xxx02`)와 BGA 라인에도 동일하게 적용. 실전 BMS 파일의 비표준 표기 대응 | 낮음. 라인 파싱 수정만 필요, 순수 문자열 조작 | `beetle-core/src/bms.rs` |
| C2 | **차트 해시 (MD5 / SHA-256)** | 현재 해시 계산 없음. 곡 식별, 스코어 키, 이후 마이그레이션·IR의 전제 조건. 해시는 호환 처리 이전의 **원본 바이트** 기준으로 계산 | 중간. 외부 크레이트 없이 직접 구현 시 약 150줄 | `beetle-core` |
| C3 | **Auto-adjust (판정 오프셋 보정)** | 플레이 중 FAST/SLOW 델타 평균으로 `judge_offset_ms`를 추천하거나 자동 적용. `record_hit_with_delta`와 오프셋 설정이 이미 존재하여 배선 중심 작업 | 낮음 | `beetle-core/src/judge`, `beetle-app/src/config.rs` |
| C4 | **FAST/SLOW ms 표시** | 카운트에 더해 판정 델타(ms) 값 자체를 HUD에 표시 | 낮음. 렌더 작업 | `beetle-render/src/screens/play.rs` |
| C5 | **LN 모드 확장** | 현재 LNTYPE 1 중심. `#LNOBJ`, LNTYPE 2 계열 점검 후 LN / CN / HCN 선택(AUTO / FORCE) 도입 | 중간. 판정·게이지 처리 분기 증가 | `beetle-core/src/bms.rs`, `judge` |

### 2.2 고려 대상 — 설계 결정 필요

| # | 후보 | 내용 | 비용 / 리스크 |
| :--- | :--- | :--- | :--- |
| C6 | **곡별 볼륨 정규화** | 로딩 시점에 PCM의 RMS/피크로 곡 단위 게인 하나를 산출해 적용. INV-3(사전 디코딩) 범위 내이며 오디오 콜백은 변경 없음 | 로딩 시간 증가. 측정 기준(RMS vs 피크) 결정 필요 |
| C7 | **프리뷰 자동 생성** | `#PREVIEW`/프리뷰 음원이 없는 곡에 대해 키음 믹스다운에서 N초 구간을 생성하여 선곡 화면에서 재생 | 오프라인 믹스다운 필요. 기존 오디오 아틀라스 재사용 가능. 백그라운드 처리(INV-5) 필수 |
| C8 | **LR2 / beatoraja 스코어 DB 마이그레이션** | 기존 플레이어의 스코어 이전. 이전 장벽을 낮춤 | 높음. 두 DB 모두 SQLite 형식이라 읽기 전용 미니 리더를 직접 구현해야 함(`rusqlite` 금지). C2(해시) 선행 필수 |
| C9 | **판정 룰 모드 프리셋** | beatoraja / LR2oraja / DX 방식의 판정창·게이지 규칙 선택. 현재는 `#RANK`만 반영 | 중간. 룰 프리셋 구조 신설 필요 |
| C10 | **`#RANDOM` / `#IF` 분기** | 일부 구형 BMS에서 사용. 현재 파서에서 처리 흔적 없음 (확인 필요) | 중간. 지원 시 호환 곡 증가 |

### 2.3 장기 / 보류

- **WASAPI Exclusive**: cpal 지원 범위 확인 후 지연 시간 개선 목적으로 검토.
- **Non-stop / Battle / Arena / Recording 모드**: bmz-player의 로드맵 항목. Beetle 범위 밖이므로 현재 제안하지 않음.

---

## 3. 후속 확인 사항

- `bpm`에 일반 BMS 폴더 / ZIP → `.bmsp` 변환 import 경로가 이미 있는지 확인. 없다면 곡 루트 직접 스캔을 대체하는 현실적 대안이 될 수 있음.
- C10: 기존 파서가 `#RANDOM` 계열을 무시하는지, 오동작하는지 테스트로 확인.
- C5: 현재 LNTYPE 2, `#LNOBJ` 처리 상태를 테스트로 확인.

## 4. 제안 착수 순서

**C1 → C2 → C3 / C4 → C5 → C6**

- C1, C2는 호환성의 토대이며 C2는 이후 C8(마이그레이션)의 전제 조건.
- C3, C4는 이미 존재하는 델타 데이터 위에 올리는 소규모 개선.
- 진행 중인 Milestone 10(멀티키 모드 Phase 4~5)을 우선 마무리한 뒤 착수한다.
