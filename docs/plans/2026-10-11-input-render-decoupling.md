# 입력·판정과 그리기 분리 계획 (2026-10-11)

상태: **진행 중** — D0 생략(2026-10-11 결정), D1·D2 완료(2026-10-11), D3 다음

대상: `crates/beetle-app`, `crates/beetle-audio`, `crates/beetle-render/src/backend/d3d11`

## 배경

리듬게임에서 판정 시각은 **키가 눌린 순간**이어야 한다. 지금 beetle-app은 키를 **처리한 순간**의
오디오 클럭을 판정 시각으로 쓰고, 그 처리는 그리기(Present)와 같은 스레드에서 차례를 기다린다.
그래서 판정 오차가 프레임레이트·vsync·GPU 부하에 따라 달라진다. 이는 INV-1(오디오 클럭 기준
결정론적 판정)과 INV-4(로직/입력 스레드와 렌더 스레드 분리)를 코드가 지키지 못하는 상태다 —
실제로는 오디오 스레드 + 모든 일을 하는 메인 스레드, 두 개뿐이다.

## 진단 (2026-10-11 코드 기준)

1. **키 입력이 Present 뒤에 줄 선다.** 레인 키는 winit `WindowEvent::KeyboardInput`
   (`crates/beetle-app/src/main.rs` `window_event`) → `handle_gameplay_input`
   (`crates/beetle-app/src/handlers/gameplay.rs`)에서 처리된다. 같은 스레드의
   `RedrawRequested`가 `present::gameplay` → `end_frame`의 `Present(1, 0)`
   (`crates/beetle-render/src/backend/d3d11/mod.rs`)에서 vblank까지 막혀 있는 동안 키 메시지는
   메시지 큐에 쌓였다가 Present가 끝난 뒤에야 꺼내진다.
2. **판정 시각 = 처리 시각.** `handle_gameplay_input`은 처리 시점에
   `clock().current_time_seconds()`를 읽어 `judged_time`에 넣는다. 1번 때문에 입력은 최대
   1프레임(60Hz에서 ~16.7ms, GPU 큐가 차 있으면 그 이상) **늦게, 들쭉날쭉하게** 찍힌다.
   리플레이(`rep.record(audio_time, …)`)에도 같은 오차가 그대로 기록된다.
3. **오디오 클럭이 버퍼 단위로 계단식이다.** `AudioClock::current_samples()`
   (`crates/beetle-audio/src/clock.rs`)는 믹서 콜백이 버퍼를 하나 채울 때마다 올리는
   `samples_played`를 그대로 돌려준다. WASAPI 공유 모드 기준 ~10ms 계단이라 입력을 즉시 처리해도
   판정 시각이 버퍼 크기만큼 양자화되고, 노트 스크롤도 같은 계단으로 떨린다.
4. **판정 틱과 BGM 발음이 프레임에 묶여 있다.** `tick_gameplay`
   (`crates/beetle-app/src/gameplay.rs`) — 미스 판정, `advance_gameplay_timelines`의 BGM
   `PlaySample`, 오토플레이·리플레이 구동 — 은 `RedrawRequested` 안에서만 돈다. 낮은 FPS 설정에서는
   BGM 키음이 프레임 간격으로 뭉쳐 울리고, 렌더가 멈추면 판정도 멈춘다.
5. **키음 지연이 프레임에 묶여 있다.** 플레이어 키음 `PlaySample`도 1번의 지연을 그대로 겪는다.
6. **스왑체인이 지연을 키운다.** `DXGI_SWAP_EFFECT_DISCARD` + `BufferCount: 2`, 프레임 지연
   상한 미설정(DXGI 기본 3). 화면 표시 지연은 판정 정확도와 별개지만 체감 지연을 늘린다.

## 목표

- **판정 시각은 입력 획득 시점의 타임스탬프로 정한다.** 처리가 얼마나 늦든 판정 결과는 같아야 한다.
- 판정·미스 처리·BGM 발음은 렌더 FPS와 무관하게 돈다. 렌더가 멈춰도 판정은 계속된다.
- 키음은 렌더를 기다리지 않고 다음 오디오 버퍼에 실린다. BGM은 샘플 단위로 정확히 울린다.
- 렌더는 판정 상태를 **읽기만** 한다.
- 새 크레이트 없음. Raw Input·QPC는 기존처럼 OS DLL 직접 FFI(ADR-026 방식).

## 목표 구조

```
[입력/로직 스레드]                           [오디오 스레드 (cpal)]
 Raw Input(WM_INPUT) + QPC 타임스탬프          믹서: 즉시 재생 + 예약 재생(start_sample)
 → AudioClock::time_at(qpc) 로 판정 시각 산출   콜백마다 (samples, qpc) 기록
 JudgeEngine 소유, 1ms 틱(미스·BGM 예약)   ──rtrb──▶  (로직 전용 Producer)
        │ rtrb: GameplayEvent (판정/키 상태/게이지)
        ▼
[메인 스레드 = winit + 렌더]
 메뉴·IME·핫키, 화면 전이
 GameplayEvent 를 읽어 미러 상태 갱신 → D3D11 Present
        │ rtrb: LogicCommand (일시정지/재개/종료/오프셋 변경)
        └──────────────▶ 입력/로직 스레드
```

INV-4의 세 스레드(오디오 / 로직·입력 / 렌더)를 그대로 실현한다. 렌더를 별도 스레드로 빼는 대신
**판정을 메인 스레드에서 빼는** 쪽을 택한다 — `AppState`의 메뉴·화면 코드는 그대로 두고,
게임플레이 판정에 필요한 것(차트, 타이밍, `JudgeEngine`, 키 바인딩 스냅샷, 오디오 Producer)만
로직 스레드로 옮기면 되기 때문이다.

## 단계

각 단계는 독립 커밋으로 끝나고, 앞 단계만으로도 효과가 있도록 순서를 잡았다.

### D0. 측정 기반 — **생략** (2026-10-11)

문제는 코드로 확정돼 있어 수정 방향을 정하는 데 측정이 필요하지 않다고 판단해 건너뛴다.
필요해지면(단계별 효과 비교, 회귀 감시) 아래 내용대로 나중에 추가한다.

- devtools에 **입력 봇** 추가: 오토플레이와 같은 노트 시각에 `SendInput`으로 실제 키 이벤트를
  주입하고, 결과 `delta_ms` 분포(평균·표준편차·최대)를 기록한다. 봇 자체 지터는 QPC로 따로 기록해
  뺀다.
- 조건 행렬: 목표 FPS 60(vsync) / 144 / 무제한, BGA 동영상 유무, WARP.
- 기대 기준선: 평균이 늦은 쪽으로 반 프레임가량 치우치고, 표준편차가 FPS에 반비례.
- 결과는 이 문서 「측정」 절에 표로 남긴다.

### D1. 오디오 클럭 보간 (`beetle-audio`) — **완료** (2026-10-11)

- 믹서 콜백 진입 시 `(samples_played, QPC)` 쌍을 시퀀스 락(AtomicU64 세 개, 쓰기는 콜백 하나뿐)으로
  게시한다. 할당·락 없음(INV-2).
- `AudioClock::time_at(qpc) -> f64`: 마지막 쌍에서 경과 QPC × 샘플레이트로 보간한다.
  다음 버퍼 경계(`samples + buffer_frames`)를 넘지 않게 자르고, 직전 반환값보다 작아지지 않게
  단조성을 보장한다. 일시정지 중에는 보간하지 않는다.
- `current_time_seconds()`는 `time_at(now)`로 바꿔 노트 스크롤 계단 떨림도 함께 없앤다.
- 출력 장치 고정 지연은 상수 오프셋이라 판정 오프셋 보정이 흡수한다. 여기서 없애는 것은 **지터**다.
- 테스트: 가짜 QPC로 보간·클램프·단조성·리셋·일시정지 동작.
- 구현 메모: 시각은 QPC 대신 `std::time::Instant`(Windows에서 QPC 기반)를 쓴다 — D3의 입력
  스레드도 수신 즉시 `Instant::now()`로 찍으면 같은 기준이 된다. 단조 보장은
  `current_time_seconds()`(지금 시각)에만 걸고, `time_at(t)`는 과거 시각을 되돌려 읽어야 하므로
  보장하지 않는다(마지막 콜백 이전 시각은 선형으로 거꾸로 읽음). `AudioClock::reset()`(메인
  스레드에서 쓰던 두 번째 쓰기 경로)은 없애고 `ResetClock` 명령으로만 되돌린다. 버퍼 단위 원값은
  `rendered_samples()`로 남겼다.

### D2. 샘플 단위 예약 재생 (`beetle-audio`) — **완료** (2026-10-11)

- `AudioCommand::PlaySampleAt { sample_id, start_sample, volume, pan }` 추가. 믹서는 고정 크기
  예약 슬롯 배열(사전 할당)에 넣었다가 버퍼 안의 정확한 오프셋에서 발음을 시작한다.
  지난 시각이면 즉시 재생.
- BGM 노트와 오토플레이 키음을 **선행 예약**(~50ms 앞)으로 바꾼다. 프레임·틱 간격과 무관해진다.
- 일시정지·정지 시 예약 슬롯을 비운다.
- 테스트: 믹서 단위 테스트로 버퍼 경계를 걸치는 예약, 슬롯 포화, 정지 시 비움.
- 구현 메모: 예약 슬롯 256개(`MAX_SCHEDULED`), 선행 시간 100ms(`SCHEDULE_AHEAD_SECONDS`,
  가장 낮은 FPS의 한 프레임보다 길게). 슬롯이 다 차면 버리지 않고 즉시 재생한다.
  **일시정지는 슬롯을 비우지 않는다** — 클럭이 멈추므로 예약도 함께 기다렸다가 재개 후 제 프레임에
  울린다(비우면 이미 넘긴 BGM이 사라진다). 비우는 것은 `StopAll`·`StopSample`·`ResetClock`.
  오토플레이 키음은 `JudgeEngine::auto_play_sounds(after, until)`로 미리 예약하고
  `auto_play_update`의 결과는 판정 표시에만 쓴다. 리플레이 재생의 키음은 판정 결과에 따라 달라지므로
  예약하지 않고 지금처럼 즉시 재생한다.

### D3. 입력 스레드 + 획득 시점 타임스탬프 (`beetle-app` 신규 `raw_input.rs`)

- 전용 스레드가 메시지 전용 창(`HWND_MESSAGE`)을 만들고 `RegisterRawInputDevices`(키보드,
  `RIDEV_INPUTSINK`)로 `WM_INPUT`을 받는다. 수신 즉시 `QueryPerformanceCounter`로 찍는다.
  (`GetMessageTime`은 ms·시스템 틱 해상도라 쓰지 않는다.)
- 포그라운드 창이 게임 창일 때만 이벤트를 낸다. 포커스를 잃으면 눌린 레인을 모두 떼는 이벤트를 낸다.
- 스캔코드 → `KeyCode`는 winit의 `PhysicalKeyExtScancode::from_scancode`를 써서 기존
  `KeyBindings`(`input.rs`)를 그대로 쓴다. E0 확장키·Pause 같은 예외는 표로 보정한다.
- Raw Input은 자동 반복이 없고 IME를 거치지 않는다 — 게임플레이에 오히려 맞다. 같은 키의 연속
  make는 `lane_transition`이 걸러낸다.
- `InputEvent { code, down, qpc }`를 rtrb로 보낸다.
- **이 단계에서는 소비를 메인 스레드에 둔다**: `about_to_wait`와 그리기 직전에 큐를 비우고,
  판정 시각을 `clock.time_at(ev.qpc)`로 계산한다. 게임플레이 중 winit의 레인 키 이벤트는 무시한다
  (ESC·F3/F4 등 핫키는 winit 유지).
- 이것만으로 **판정 정확도 문제(진단 1–3)는 해결**된다. 처리가 늦어도 판정 시각은 바뀌지 않는다.
  리플레이도 획득 시각을 기록한다. 남는 것은 키음·화면 피드백 지연(진단 5).
- 판정 오프셋 보정(`calibration.rs`)도 같은 경로로 바꾼다. 늦은 쪽 편향이 사라지므로 기존 사용자의
  오프셋 값이 어긋난다 → 첫 실행 시 「판정 오프셋을 다시 측정해 주세요」 토스트를 한 번 띄운다.

### D4. 판정을 로직 스레드로 이전

- D3의 입력 스레드를 **입력/로직 스레드**로 키운다. 루프는 `MsgWaitForMultipleObjects`(타임아웃
  1ms)로 `WM_INPUT`과 틱을 함께 기다린다(`timeBeginPeriod(1)`은 이미 켜져 있음).
- 곡 시작(`finalize_start_gameplay`) 때 `GameplaySession { chart, timing, judge, bindings,
  options, audio_producer }`를 로직 스레드로 **넘긴다**(move). 곡 끝에 `PlayResult`·리플레이·
  게이지 추이를 돌려받는다.
- 오디오: 믹서가 Producer 두 개를 소비하도록 바꾼다(메인용 / 로직용). cpal 스트림을 가진
  `AudioEngine`은 메인 스레드에 그대로 둔다.
- 로직 스레드가 하는 일: 레인 입력 판정, 키음 즉시 발음, `update_misses`, BGM·오토플레이 선행 예약
  (D2), 리플레이 구동, 게이지 표본, 실패·종료 판정.
- 로직 → 렌더: `GameplayEvent`(판정 등급·delta·레인, 키 눌림/뗌, 노트 처리 인덱스, 점수·게이지
  스냅샷) rtrb. 렌더는 이를 받아 `state.view`와 노트 처리 비트셋 미러를 갱신한다. 노트 위치는
  여전히 `AudioClock`만으로 계산한다(INV-1).
- 렌더 → 로직: `LogicCommand`(일시정지/재개/재시작/종료, 오프셋 변경) rtrb.
- 큐가 차면 로직은 이벤트를 버리지 않고 점수 스냅샷으로 합친다(판정 결과의 진실은 로직 쪽에 있다).
- `tick_gameplay`는 순수 함수로 남겨 로직 스레드와 테스트가 같이 쓴다.
- 테스트: 같은 입력 이벤트열을 넣으면 렌더 FPS·큐 소비 시점과 무관하게 같은 `PlayResult`가 나오는지
  (결정론), 리플레이 재생 결과 일치, 일시정지 중 뗀 키 처리.

### D5. 표시 지연 줄이기 + 문서

- 스왑체인을 `DXGI_SWAP_EFFECT_FLIP_DISCARD`(버퍼 2)로 바꾸고
  `IDXGISwapChain2::SetMaximumFrameLatency(1)` + 프레임 지연 대기 객체로, 그리기 직전에 기다렸다가
  최신 클럭으로 그린다. 무제한 FPS는 `DXGI_PRESENT_ALLOW_TEARING`(지원 시).
- 메인 루프의 화면별 `ControlFlow` 분기에서 게임플레이 분기는 렌더 페이싱만 남긴다.
- (D0을 생략했으므로 전/후 측정은 하지 않는다.)
- `docs/DECISIONS.md`에 ADR-028(입력 획득 시점 판정, Raw Input 입력/로직 스레드)을 쓰고,
  `AGENTS.md` INV-4의 설명을 실제 구조에 맞게 고친다.

## 위험과 대응

| 위험 | 대응 |
| --- | --- |
| `RIDEV_INPUTSINK`로 다른 창 포커스 중 입력도 들어옴 | 포그라운드 창 검사, 포커스 잃으면 전부 뗌 |
| 스캔코드 매핑 누락(확장키, 일부 키보드) | 매핑 표 + 단위 테스트, 매핑 실패 키는 winit 경로로 폴백 |
| 보간 클럭이 버퍼 경계에서 뒤로 감 | 클램프 + 단조 보장, 테스트로 고정 |
| 사용자 판정 오프셋이 바뀐 구조와 어긋남 | 1회 재측정 안내 토스트 |
| 로직·렌더 상태 불일치(노트가 그려졌다 사라짐 등) | 렌더 미러는 로직 이벤트로만 갱신, 결정론 테스트 |
| 2개 Producer로 명령 순서 꼬임 | 메인용은 전역 제어(정지/일시정지), 로직용은 발음·예약만 — 역할을 겹치지 않게 |

## 범위 밖

- 컨트롤러(HID 게임패드·IIDX 전용 컨트롤러) 입력. D3의 Raw Input 구조에 HID 장치 등록만 더하면
  되도록 열어 두되 이번에는 키보드만.
- WASAPI 독점 모드·저지연 버퍼 크기 설정(오디오 출력 지연 자체를 줄이는 일).

## 측정

(D0 생략 — 측정하지 않음)
