//! Xbox-style controllers (XInput) for menu navigation.
//!
//! `xinput1_4.dll` is loaded at run time (then the older `xinput1_3.dll` and
//! `xinput9_1_0.dll`), so the exe starts without any of them. The render
//! thread polls once per frame; while no pad is connected the slots are
//! scanned at most once a second, since `XInputGetState` is slow on empty
//! slots.
//!
//! The button edges and stick directions become `MenuInput`s here. The game
//! feeds each one to the keyboard handler of the same action (`menu_key`),
//! so every screen treats a pad exactly like the key it stands for. Lane
//! keys are never produced from a pad (only Start, which pauses).

use std::ffi::c_void;
use std::time::{Duration, Instant};

use winit::keyboard::KeyCode;

pub const BTN_DPAD_UP: u16 = 0x0001;
pub const BTN_DPAD_DOWN: u16 = 0x0002;
pub const BTN_DPAD_LEFT: u16 = 0x0004;
pub const BTN_DPAD_RIGHT: u16 = 0x0008;
pub const BTN_START: u16 = 0x0010;
pub const BTN_BACK: u16 = 0x0020;
pub const BTN_LEFT_SHOULDER: u16 = 0x0100;
pub const BTN_RIGHT_SHOULDER: u16 = 0x0200;
pub const BTN_A: u16 = 0x1000;
pub const BTN_B: u16 = 0x2000;
pub const BTN_X: u16 = 0x4000;
pub const BTN_Y: u16 = 0x8000;

/// Stick magnitude (0..1) that starts a direction.
pub const STICK_ENTER: f32 = 0.35;
/// Magnitude below which a held direction lets go (hysteresis).
pub const STICK_EXIT: f32 = 0.25;
/// Trigger position (of 255) that counts as a press.
pub const TRIGGER_PRESS: u8 = 128;
/// Delay before the first repeat of a held direction, then between repeats.
pub const REPEAT_FIRST: Duration = Duration::from_millis(350);
pub const REPEAT_NEXT: Duration = Duration::from_millis(90);
/// How often an empty set of slots is scanned.
const SCAN_INTERVAL: Duration = Duration::from_secs(1);
/// After a pad button binds a key, the pad's menu events are dropped this
/// long: the same press also reaches XInput, a moment after the raw input.
pub const BIND_QUIET: Duration = Duration::from_millis(250);

/// The state of one pad, as the menus need it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PadState {
    pub buttons: u16,
    pub left_x: i16,
    pub left_y: i16,
    pub left_trigger: u8,
    pub right_trigger: u8,
}

/// An action a pad asks of the menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuInput {
    Up,
    Down,
    Left,
    Right,
    /// A: confirm (Enter).
    Confirm,
    /// B: cancel (Escape).
    Cancel,
    /// X: auto play toggle (A on the keyboard, Song Select).
    Auto,
    /// Y: play options (Tab).
    Options,
    /// Start: Enter in menus, pause (Escape) during a play.
    Start,
    /// Back: settings (F4).
    Settings,
    /// LB / RB: page up / down.
    PageUp,
    PageDown,
    /// LT / RT: folder previous / next (F1 / F3).
    FolderPrev,
    FolderNext,
}

/// Which screen the inputs go to, decided by the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadContext {
    Menu,
    /// Playing: only Start (pause) reaches the game.
    Play,
    /// The pause menu of a play.
    PausedPlay,
}

/// Button bit to input, in the order they are reported on one frame.
const BUTTONS: [(u16, MenuInput); 8] = [
    (BTN_A, MenuInput::Confirm),
    (BTN_B, MenuInput::Cancel),
    (BTN_X, MenuInput::Auto),
    (BTN_Y, MenuInput::Options),
    (BTN_START, MenuInput::Start),
    (BTN_BACK, MenuInput::Settings),
    (BTN_LEFT_SHOULDER, MenuInput::PageUp),
    (BTN_RIGHT_SHOULDER, MenuInput::PageDown),
];

/// The key a menu input acts as in the menus.
pub fn menu_key(input: MenuInput) -> KeyCode {
    match input {
        MenuInput::Up => KeyCode::ArrowUp,
        MenuInput::Down => KeyCode::ArrowDown,
        MenuInput::Left => KeyCode::ArrowLeft,
        MenuInput::Right => KeyCode::ArrowRight,
        MenuInput::Confirm | MenuInput::Start => KeyCode::Enter,
        MenuInput::Cancel => KeyCode::Escape,
        MenuInput::Auto => KeyCode::KeyA,
        MenuInput::Options => KeyCode::Tab,
        MenuInput::Settings => KeyCode::F4,
        MenuInput::PageUp => KeyCode::PageUp,
        MenuInput::PageDown => KeyCode::PageDown,
        MenuInput::FolderPrev => KeyCode::F1,
        MenuInput::FolderNext => KeyCode::F3,
    }
}

/// The key a pad input acts as in the given context, or `None` when the
/// input means nothing there.
pub fn key_for(input: MenuInput, context: PadContext) -> Option<KeyCode> {
    match context {
        PadContext::Menu => Some(menu_key(input)),
        PadContext::Play => (input == MenuInput::Start).then_some(KeyCode::Escape),
        PadContext::PausedPlay => match input {
            MenuInput::Start | MenuInput::Cancel => Some(KeyCode::Escape),
            MenuInput::Up => Some(KeyCode::ArrowUp),
            MenuInput::Down => Some(KeyCode::ArrowDown),
            MenuInput::Confirm => Some(KeyCode::Enter),
            _ => None,
        },
    }
}

/// One held direction: its value (-1, 0, +1) and when it next repeats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Repeat {
    dir: i8,
    next: Option<Instant>,
}

/// Repeat and stick hysteresis state carried from frame to frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuRepeat {
    x: Repeat,
    y: Repeat,
    stick_x_on: bool,
    stick_y_on: bool,
}

/// The direction of a stick value, with hysteresis: it starts beyond
/// `STICK_ENTER` and stops inside `STICK_EXIT`. +1 is right / up.
fn stick_dir(value: i16, engaged: &mut bool) -> i8 {
    let magnitude = (f32::from(value) / 32767.0).clamp(-1.0, 1.0);
    let size = magnitude.abs();
    let on = if *engaged {
        size >= STICK_EXIT
    } else {
        size >= STICK_ENTER
    };
    *engaged = on;
    match (on, magnitude >= 0.0) {
        (false, _) => 0,
        (true, true) => 1,
        (true, false) => -1,
    }
}

/// Moves one axis: a change of direction presses it, a held direction
/// repeats after `REPEAT_FIRST`, then every `REPEAT_NEXT`.
fn step(
    axis: &mut Repeat,
    dir: i8,
    at: Instant,
    neg: MenuInput,
    pos: MenuInput,
    out: &mut Vec<MenuInput>,
) {
    let input = |d: i8| if d > 0 { pos } else { neg };
    if dir != axis.dir {
        axis.dir = dir;
        axis.next = None;
        if dir != 0 {
            out.push(input(dir));
            axis.next = Some(at + REPEAT_FIRST);
        }
    } else if let Some(due) = axis.next {
        if dir != 0 && at >= due {
            out.push(input(dir));
            axis.next = Some(at + REPEAT_NEXT);
        }
    }
}

/// The menu inputs of one pad report, given the previous report.
///
/// `at` is the moment the report was read; repeats are timed from it, so the
/// function is deterministic for injected instants. Dpad wins over the left
/// stick when both point somewhere.
pub fn pad_to_menu_events(
    prev: PadState,
    now: PadState,
    repeat: &mut MenuRepeat,
    at: Instant,
) -> Vec<MenuInput> {
    let mut out = Vec::new();

    let stick_x = stick_dir(now.left_x, &mut repeat.stick_x_on);
    let stick_y = stick_dir(now.left_y, &mut repeat.stick_y_on);
    let dpad_x =
        i8::from(now.buttons & BTN_DPAD_RIGHT != 0) - i8::from(now.buttons & BTN_DPAD_LEFT != 0);
    let dpad_y =
        i8::from(now.buttons & BTN_DPAD_UP != 0) - i8::from(now.buttons & BTN_DPAD_DOWN != 0);
    let dir_x = if dpad_x != 0 { dpad_x } else { stick_x };
    let dir_y = if dpad_y != 0 { dpad_y } else { stick_y };
    step(
        &mut repeat.x,
        dir_x,
        at,
        MenuInput::Left,
        MenuInput::Right,
        &mut out,
    );
    step(
        &mut repeat.y,
        dir_y,
        at,
        MenuInput::Down,
        MenuInput::Up,
        &mut out,
    );

    let pressed = now.buttons & !prev.buttons;
    for (bit, input) in BUTTONS {
        if pressed & bit != 0 {
            out.push(input);
        }
    }
    if prev.left_trigger < TRIGGER_PRESS && now.left_trigger >= TRIGGER_PRESS {
        out.push(MenuInput::FolderPrev);
    }
    if prev.right_trigger < TRIGGER_PRESS && now.right_trigger >= TRIGGER_PRESS {
        out.push(MenuInput::FolderNext);
    }
    out
}

// --- the DLL -------------------------------------------------------------

/// `XINPUT_STATE` as the DLL fills it (16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct XInputState {
    packet: u32,
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    left_x: i16,
    left_y: i16,
    right_x: i16,
    right_y: i16,
}

type GetStateFn = unsafe extern "system" fn(u32, *mut XInputState) -> u32;
const ERROR_SUCCESS: u32 = 0;

/// The DLLs tried in order, and the ordinal the legacy ones export
/// `XInputGetState` under when the name is missing.
const DLLS: [&str; 3] = ["xinput1_4.dll", "xinput1_3.dll", "xinput9_1_0.dll"];
const LEGACY_ORDINAL: usize = 5;

/// The events a poll hands on: all of them when they are accepted and no
/// suppression is running at `at`, none otherwise.
fn keep_events(
    events: Vec<MenuInput>,
    accept: bool,
    suppressed_until: Option<Instant>,
    at: Instant,
) -> Vec<MenuInput> {
    let suppressed = suppressed_until.is_some_and(|until| at < until);
    if accept && !suppressed {
        events
    } else {
        Vec::new()
    }
}

/// Whether an empty-slot scan is due at `at`, given the last one.
fn scan_due(last_scan: Option<Instant>, at: Instant) -> bool {
    last_scan.is_none_or(|last| at.saturating_duration_since(last) >= SCAN_INTERVAL)
}

/// The slot probed on this scan tick, and the one after it. Empty slots are
/// probed one per tick in turn, so a scan costs one `XInputGetState` call.
fn probe_slot(next: &mut u32) -> u32 {
    let slot = *next;
    *next = (slot + 1) % XINPUT_SLOTS;
    slot
}

/// The XInput user slots.
const XINPUT_SLOTS: u32 = 4;

/// Polls the controller slots. Lives on the render thread.
pub struct XInputPad {
    get_state: GetStateFn,
    slot: Option<u32>,
    prev: PadState,
    repeat: MenuRepeat,
    last_scan: Option<Instant>,
    /// The slot the next empty-slot scan probes.
    next_probe: u32,
    /// Menu events are dropped until this instant (see `BIND_QUIET`).
    suppressed_until: Option<Instant>,
}

impl XInputPad {
    /// `None` when no XInput DLL can be loaded (no controller support then).
    pub fn load() -> Option<Self> {
        let get_state = unsafe { load_get_state()? };
        Some(Self {
            get_state,
            slot: None,
            prev: PadState::default(),
            repeat: MenuRepeat::default(),
            last_scan: None,
            next_probe: 0,
            suppressed_until: None,
        })
    }

    /// Whether a pad is connected (its slot is known).
    pub fn is_connected(&self) -> bool {
        self.slot.is_some()
    }

    /// When the next empty-slot scan is due: `now` when it is overdue.
    pub fn next_scan(&self, now: Instant) -> Instant {
        self.last_scan
            .map_or(now, |last| (last + SCAN_INTERVAL).max(now))
    }

    /// Drops the pad's menu events until `until`. The state keeps following
    /// the pad, so the press itself is not seen again afterwards.
    pub fn suppress_menu_until(&mut self, until: Instant) {
        self.suppressed_until = Some(until);
    }

    /// Takes the pad's current state as the baseline without events. Called
    /// when the window gets the focus back: while it had none, the pad was
    /// not polled, so its changes then are not presses.
    pub fn resync(&mut self) {
        self.repeat = MenuRepeat::default();
        if let Some(slot) = self.slot {
            match self.read(slot) {
                Some(now) => self.prev = now,
                None => {
                    self.slot = None;
                    self.prev = PadState::default();
                }
            }
        }
    }

    /// The menu inputs since the last poll, for the pad that is connected.
    ///
    /// The state always advances, even when `accept` is false (rebinding, an
    /// unfocused window, a screen that takes no pad), so the next accepted
    /// poll sees only real changes; the events of the unaccepted polls are
    /// dropped. With no pad connected, one slot is probed per second when
    /// `scan` is set (a play passes `false`: an empty slot read can take
    /// milliseconds).
    pub fn poll(&mut self, at: Instant, scan: bool, accept: bool) -> Vec<MenuInput> {
        let events = self.advance(at, scan);
        keep_events(events, accept, self.suppressed_until, at)
    }

    fn advance(&mut self, at: Instant, scan: bool) -> Vec<MenuInput> {
        let Some(slot) = self.slot else {
            if scan && scan_due(self.last_scan, at) {
                self.last_scan = Some(at);
                let probe = probe_slot(&mut self.next_probe);
                if let Some(now) = self.read(probe) {
                    self.slot = Some(probe);
                    // Buttons already down on connect are not presses.
                    self.prev = now;
                }
            }
            return Vec::new();
        };
        match self.read(slot) {
            Some(now) => {
                let events = pad_to_menu_events(self.prev, now, &mut self.repeat, at);
                self.prev = now;
                events
            }
            None => {
                self.slot = None;
                self.prev = PadState::default();
                self.repeat = MenuRepeat::default();
                self.last_scan = Some(at);
                Vec::new()
            }
        }
    }

    fn read(&self, slot: u32) -> Option<PadState> {
        let mut state = XInputState::default();
        let result = unsafe { (self.get_state)(slot, &mut state) };
        (result == ERROR_SUCCESS).then_some(PadState {
            buttons: state.buttons,
            left_x: state.left_x,
            left_y: state.left_y,
            left_trigger: state.left_trigger,
            right_trigger: state.right_trigger,
        })
    }
}

/// Loads the first XInput DLL that exports `XInputGetState`.
unsafe fn load_get_state() -> Option<GetStateFn> {
    for name in DLLS {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let module = LoadLibraryW(wide.as_ptr());
        if module.is_null() {
            continue;
        }
        let mut proc = GetProcAddress(module, c"XInputGetState".as_ptr().cast());
        if proc.is_null() {
            proc = GetProcAddress(module, LEGACY_ORDINAL as *const std::ffi::c_char);
        }
        if !proc.is_null() {
            return Some(std::mem::transmute::<*mut c_void, GetStateFn>(proc));
        }
    }
    None
}

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const std::ffi::c_char) -> *mut c_void;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    fn pad(buttons: u16) -> PadState {
        PadState {
            buttons,
            ..PadState::default()
        }
    }

    fn stick(x: i16, y: i16) -> PadState {
        PadState {
            left_x: x,
            left_y: y,
            ..PadState::default()
        }
    }

    #[test]
    fn xinput_state_matches_the_windows_layout() {
        assert_eq!(std::mem::size_of::<XInputState>(), 16);
        assert_eq!(std::mem::offset_of!(XInputState, buttons), 4);
        assert_eq!(std::mem::offset_of!(XInputState, left_x), 8);
    }

    #[test]
    fn each_button_maps_to_its_action_on_press_only() {
        let at = t0();
        let cases = [
            (BTN_A, MenuInput::Confirm),
            (BTN_B, MenuInput::Cancel),
            (BTN_X, MenuInput::Auto),
            (BTN_Y, MenuInput::Options),
            (BTN_START, MenuInput::Start),
            (BTN_BACK, MenuInput::Settings),
            (BTN_LEFT_SHOULDER, MenuInput::PageUp),
            (BTN_RIGHT_SHOULDER, MenuInput::PageDown),
        ];
        for (bit, input) in cases {
            let mut rep = MenuRepeat::default();
            assert_eq!(
                pad_to_menu_events(PadState::default(), pad(bit), &mut rep, at),
                vec![input]
            );
            // Held: no second press.
            assert!(pad_to_menu_events(pad(bit), pad(bit), &mut rep, at).is_empty());
            // Released: nothing.
            assert!(pad_to_menu_events(pad(bit), PadState::default(), &mut rep, at).is_empty());
        }
    }

    #[test]
    fn triggers_step_folders_on_the_press_edge() {
        let at = t0();
        let mut rep = MenuRepeat::default();
        let lt = PadState {
            left_trigger: 200,
            ..PadState::default()
        };
        let rt = PadState {
            right_trigger: 200,
            ..PadState::default()
        };
        assert_eq!(
            pad_to_menu_events(PadState::default(), lt, &mut rep, at),
            vec![MenuInput::FolderPrev]
        );
        assert_eq!(
            pad_to_menu_events(PadState::default(), rt, &mut rep, at),
            vec![MenuInput::FolderNext]
        );
        // Still pulled: no repeat.
        assert!(pad_to_menu_events(lt, lt, &mut rep, at + REPEAT_FIRST).is_empty());
    }

    #[test]
    fn dpad_moves_and_repeats_after_350_then_90_ms() {
        let t = t0();
        let mut rep = MenuRepeat::default();
        let down = pad(BTN_DPAD_DOWN);
        let idle = PadState::default();
        assert_eq!(
            pad_to_menu_events(idle, down, &mut rep, t),
            vec![MenuInput::Down]
        );
        assert!(
            pad_to_menu_events(down, down, &mut rep, t + Duration::from_millis(349)).is_empty()
        );
        assert_eq!(
            pad_to_menu_events(down, down, &mut rep, t + Duration::from_millis(350)),
            vec![MenuInput::Down]
        );
        assert!(
            pad_to_menu_events(down, down, &mut rep, t + Duration::from_millis(439)).is_empty()
        );
        assert_eq!(
            pad_to_menu_events(down, down, &mut rep, t + Duration::from_millis(440)),
            vec![MenuInput::Down]
        );
        // Let go: the repeat stops.
        assert!(
            pad_to_menu_events(down, idle, &mut rep, t + Duration::from_millis(900)).is_empty()
        );
        assert!(
            pad_to_menu_events(idle, idle, &mut rep, t + Duration::from_millis(1000)).is_empty()
        );
    }

    #[test]
    fn a_long_gap_repeats_once_not_in_a_burst() {
        let t = t0();
        let mut rep = MenuRepeat::default();
        let right = pad(BTN_DPAD_RIGHT);
        pad_to_menu_events(PadState::default(), right, &mut rep, t);
        let late = t + Duration::from_secs(5);
        assert_eq!(
            pad_to_menu_events(right, right, &mut rep, late),
            vec![MenuInput::Right]
        );
        assert!(pad_to_menu_events(right, right, &mut rep, late).is_empty());
    }

    #[test]
    fn stick_up_is_positive_y_and_left_is_negative_x() {
        let t = t0();
        let mut rep = MenuRepeat::default();
        assert_eq!(
            pad_to_menu_events(PadState::default(), stick(0, 30000), &mut rep, t),
            vec![MenuInput::Up]
        );
        let mut rep = MenuRepeat::default();
        assert_eq!(
            pad_to_menu_events(PadState::default(), stick(-30000, 0), &mut rep, t),
            vec![MenuInput::Left]
        );
    }

    #[test]
    fn stick_deadzone_has_hysteresis() {
        let t = t0();
        let mut rep = MenuRepeat::default();
        let v = |f: f32| (f * 32767.0) as i16;
        // Below the enter threshold: nothing.
        assert!(pad_to_menu_events(PadState::default(), stick(0, v(0.30)), &mut rep, t).is_empty());
        // Past it: a press.
        assert_eq!(
            pad_to_menu_events(stick(0, v(0.30)), stick(0, v(0.36)), &mut rep, t),
            vec![MenuInput::Up]
        );
        // Eases back to 0.30: still held, so no new press and no release.
        assert!(pad_to_menu_events(stick(0, v(0.36)), stick(0, v(0.30)), &mut rep, t).is_empty());
        // Under the exit threshold: released, and a new press needs 0.35 again.
        assert!(pad_to_menu_events(stick(0, v(0.30)), stick(0, v(0.20)), &mut rep, t).is_empty());
        assert!(!rep.stick_y_on);
        assert_eq!(
            pad_to_menu_events(stick(0, v(0.20)), stick(0, v(0.36)), &mut rep, t),
            vec![MenuInput::Up]
        );
    }

    #[test]
    fn dpad_overrides_the_stick_direction() {
        let t = t0();
        let mut rep = MenuRepeat::default();
        let both = PadState {
            buttons: BTN_DPAD_LEFT,
            left_x: 30000,
            ..PadState::default()
        };
        assert_eq!(
            pad_to_menu_events(PadState::default(), both, &mut rep, t),
            vec![MenuInput::Left]
        );
    }

    #[test]
    fn dropped_events_are_dropped_by_refusal_or_suppression_only() {
        let t = t0();
        let events = vec![MenuInput::Confirm];
        assert_eq!(keep_events(events.clone(), true, None, t), events);
        assert!(keep_events(events.clone(), false, None, t).is_empty());
        let until = Some(t + BIND_QUIET);
        assert!(
            keep_events(events.clone(), true, until, t + Duration::from_millis(249)).is_empty()
        );
        assert_eq!(
            keep_events(events.clone(), true, until, t + BIND_QUIET),
            events
        );
    }

    #[test]
    fn a_button_held_through_a_refused_poll_is_not_a_new_press() {
        // The poll keeps the state current while rebinding: the held A is
        // seen (and dropped), so when the prompt closes it is not a new press.
        let t = t0();
        let mut rep = MenuRepeat::default();
        let mut prev = PadState::default();
        let a = pad(BTN_A);
        let refused = pad_to_menu_events(prev, a, &mut rep, t);
        assert_eq!(keep_events(refused, false, None, t), Vec::new());
        prev = a;
        assert!(pad_to_menu_events(prev, a, &mut rep, t).is_empty());
    }

    #[test]
    fn the_empty_slot_scan_is_once_a_second() {
        let t = t0();
        assert!(scan_due(None, t));
        assert!(!scan_due(Some(t), t + Duration::from_millis(999)));
        assert!(scan_due(Some(t), t + SCAN_INTERVAL));
    }

    #[test]
    fn each_scan_tick_probes_the_next_slot_in_turn() {
        let mut next = 0;
        let visited: Vec<u32> = (0..6).map(|_| probe_slot(&mut next)).collect();
        assert_eq!(visited, [0, 1, 2, 3, 0, 1]);
    }

    #[test]
    fn menu_keys_are_the_keyboard_actions() {
        assert_eq!(menu_key(MenuInput::Confirm), KeyCode::Enter);
        assert_eq!(menu_key(MenuInput::Cancel), KeyCode::Escape);
        assert_eq!(menu_key(MenuInput::Auto), KeyCode::KeyA);
        assert_eq!(menu_key(MenuInput::Options), KeyCode::Tab);
        assert_eq!(menu_key(MenuInput::Settings), KeyCode::F4);
        assert_eq!(menu_key(MenuInput::FolderPrev), KeyCode::F1);
        assert_eq!(menu_key(MenuInput::FolderNext), KeyCode::F3);
        assert_eq!(menu_key(MenuInput::PageUp), KeyCode::PageUp);
    }

    #[test]
    fn during_a_play_only_start_reaches_the_game() {
        assert_eq!(
            key_for(MenuInput::Start, PadContext::Play),
            Some(KeyCode::Escape)
        );
        assert_eq!(key_for(MenuInput::Confirm, PadContext::Play), None);
        assert_eq!(key_for(MenuInput::Left, PadContext::Play), None);
        assert_eq!(
            key_for(MenuInput::Down, PadContext::PausedPlay),
            Some(KeyCode::ArrowDown)
        );
        assert_eq!(key_for(MenuInput::Auto, PadContext::PausedPlay), None);
    }
}
