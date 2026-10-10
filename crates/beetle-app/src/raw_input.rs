//! Keyboard input read on its own thread, stamped the moment it arrives,
//! and the judge of a play (`lane_logic`), run on the same thread.
//!
//! winit hands key events to the main thread, which also presents frames:
//! a key pressed while `Present` waits for vblank sits in the queue until
//! the frame is out. This thread takes the keyboard's raw input
//! (`WM_INPUT`) on a message-only window and stamps each key with
//! `Instant::now()` as it arrives. During a play it judges the key at once
//! on the audio clock at that instant (`AudioClock::time_at`) and starts its
//! key sound, and checks for misses every `TICK_MILLIS`; the game replays
//! the judge calls (`LaneEvent`) on its copy of the engine. Otherwise keys
//! are queued for the game as they are (`RawKey`).
//!
//! Raw input is registered once per process, so winit's own registration is
//! switched off first (`DeviceEvents::Never`); winit's ordinary key events
//! are unaffected.

use std::ffi::c_void;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use rtrb::{Consumer, Producer, RingBuffer};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::scancode::PhysicalKeyExtScancode;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::lane_logic::{LaneEvent, LaneSession, LogicCommand, TICK_MILLIS};

/// Key changes held for the game; far more than come in between two frames.
const QUEUE_CAPACITY: usize = 1024;
/// Judge calls held for the game: a frame's worth even at a few frames a second.
const EVENT_CAPACITY: usize = 4096;
const COMMAND_CAPACITY: usize = 64;
/// Room kept for keys: with less free, the miss check waits. Every call the
/// session makes must reach the game (it makes them all again on its copy),
/// so a stalled game must not fill the queue with checks; a later check
/// counts the same misses.
const TICK_HEADROOM: usize = 256;

/// One key going down or up, at the moment it reached the process.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawKey {
    pub code: KeyCode,
    pub down: bool,
    pub at: Instant,
}

/// The game's end of the queues. The thread lives as long as the process.
pub struct RawKeyboard {
    rx: Consumer<RawKey>,
    events: Consumer<LaneEvent>,
    commands: Producer<LogicCommand>,
    /// The thread's window, posted to so it takes a command at once.
    target: isize,
}

impl RawKeyboard {
    /// Starts the reading thread for the game window `hwnd`. Keys go down
    /// only while that window is in front; they always come up, so a key let
    /// go elsewhere is not left held. `None` when raw input cannot be set up
    /// (the game then judges winit's key events as before).
    pub fn spawn(hwnd: *mut c_void) -> Option<Self> {
        let (tx, rx) = RingBuffer::new(QUEUE_CAPACITY);
        let (events_tx, events) = RingBuffer::new(EVENT_CAPACITY);
        let (commands, commands_rx) = RingBuffer::new(COMMAND_CAPACITY);
        let (ready_tx, ready_rx) = mpsc::channel();
        let game = hwnd as isize;
        thread::Builder::new()
            .name("raw-input".into())
            .spawn(move || {
                let thread = Thread {
                    game: game as *mut c_void,
                    keys: tx,
                    events: events_tx,
                    commands: commands_rx,
                    session: None,
                    held: Held::default(),
                };
                thread.run(ready_tx)
            })
            .ok()?;
        let target = ready_rx.recv().ok()??;
        Some(Self {
            rx,
            events,
            commands,
            target,
        })
    }

    /// [`Self::spawn`] for a winit window.
    pub fn spawn_for(window: &Window) -> Option<Self> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::Win32(win32) = handle.as_raw() else {
            return None;
        };
        Self::spawn(win32.hwnd.get() as *mut c_void)
    }

    /// The next key change outside a play, oldest first.
    pub fn pop(&mut self) -> Option<RawKey> {
        self.rx.pop().ok()
    }

    /// The next judge call of a play, oldest first.
    pub fn pop_event(&mut self) -> Option<LaneEvent> {
        self.events.pop().ok()
    }

    /// Hands the thread a command and wakes it. `false` when the queue is full.
    pub fn send(&mut self, command: LogicCommand) -> bool {
        if self.commands.push(command).is_err() {
            return false;
        }
        unsafe {
            PostMessageW(self.target as *mut c_void, WM_APP, 0, 0);
        }
        true
    }
}

/// Keys held down, so the keyboard's auto-repeat makes no new presses.
#[derive(Default)]
struct Held(Vec<KeyCode>);

impl Held {
    /// Whether the change is news: a press of a key not held, or a release
    /// of one that is.
    fn change(&mut self, code: KeyCode, down: bool) -> bool {
        let held = self.0.iter().position(|&k| k == code);
        match (down, held) {
            (true, None) => {
                self.0.push(code);
                true
            }
            (false, Some(i)) => {
                self.0.swap_remove(i);
                true
            }
            _ => false,
        }
    }
}

struct Thread {
    game: *mut c_void,
    keys: Producer<RawKey>,
    events: Producer<LaneEvent>,
    commands: Consumer<LogicCommand>,
    session: Option<Box<LaneSession>>,
    held: Held,
}

impl Thread {
    fn run(mut self, ready: mpsc::Sender<Option<isize>>) {
        let Some(target) = (unsafe { open_target() }) else {
            let _ = ready.send(None);
            return;
        };
        let _ = ready.send(Some(target as isize));
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        }

        let tick = Duration::from_millis(u64::from(TICK_MILLIS));
        let mut msg = Msg::default();
        let mut next_tick = Instant::now();
        loop {
            // Asleep until input or a command, except for the miss checks of a play.
            let wait = if self.session.is_some() {
                let left = next_tick.saturating_duration_since(Instant::now());
                left.as_millis().min(u128::from(TICK_MILLIS)) as u32
            } else {
                INFINITE
            };
            unsafe {
                MsgWaitForMultipleObjects(0, std::ptr::null(), 0, wait, QS_ALLINPUT);
            }
            while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
                if msg.message == WM_QUIT {
                    return;
                }
                if msg.message == WM_INPUT {
                    let at = Instant::now();
                    if let Some((code, down)) = unsafe { read_key(msg.l_param) } {
                        self.key(code, down, at);
                    }
                }
                // Raw input needs DefWindowProc to release its buffers.
                unsafe { DispatchMessageW(&msg) };
            }
            while let Ok(command) = self.commands.pop() {
                self.command(command);
            }
            let now = Instant::now();
            if now >= next_tick {
                next_tick = now + tick;
                let room = self.events.slots() > TICK_HEADROOM;
                if let (Some(session), true) = (&mut self.session, room) {
                    let audio_time = session.clock().current_time_seconds();
                    if let Some(event) = session.tick(audio_time) {
                        let _ = self.events.push(event);
                    }
                }
            }
        }
    }

    fn key(&mut self, code: KeyCode, down: bool, at: Instant) {
        let in_front = unsafe { GetForegroundWindow() } == self.game;
        if (down && !in_front) || !self.held.change(code, down) {
            return;
        }
        match &mut self.session {
            Some(session) => {
                let audio_time = session.clock().time_at(at);
                if let Some(event) = session.key(code, down, audio_time) {
                    let _ = self.events.push(event);
                }
            }
            None => {
                let _ = self.keys.push(RawKey { code, down, at });
            }
        }
    }

    fn command(&mut self, command: LogicCommand) {
        match command {
            LogicCommand::Start(session) => self.session = Some(session),
            LogicCommand::End => self.session = None,
            LogicCommand::Pause(paused) => {
                if let Some(s) = &mut self.session {
                    s.set_paused(paused);
                }
            }
            LogicCommand::Offset(ms) => {
                if let Some(s) = &mut self.session {
                    s.set_offset(ms);
                }
            }
        }
    }
}

/// A message-only window registered as the process's keyboard raw input
/// target.
unsafe fn open_target() -> Option<*mut c_void> {
    let class: Vec<u16> = "STATIC".encode_utf16().chain(Some(0)).collect();
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        std::ptr::null(),
        0,
        0,
        0,
        0,
        0,
        HWND_MESSAGE,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    if hwnd.is_null() {
        return None;
    }
    let device = RawInputDevice {
        usage_page: 0x01, // generic desktop
        usage: 0x06,      // keyboard
        flags: RIDEV_INPUTSINK,
        target: hwnd,
    };
    let ok = RegisterRawInputDevices(&device, 1, std::mem::size_of::<RawInputDevice>() as u32);
    (ok != 0).then_some(hwnd)
}

/// The key and direction of a keyboard `WM_INPUT`, or `None` for anything
/// that is not one key going down or up.
unsafe fn read_key(l_param: isize) -> Option<(KeyCode, bool)> {
    let mut buf = [0u64; 8];
    let mut size = std::mem::size_of_val(&buf) as u32;
    let header = std::mem::size_of::<RawInputHeader>() as u32;
    let read = GetRawInputData(
        l_param as *mut c_void,
        RID_INPUT,
        buf.as_mut_ptr().cast(),
        &mut size,
        header,
    );
    if read == u32::MAX || read < header + std::mem::size_of::<RawKeyboardData>() as u32 {
        return None;
    }
    let head = &*(buf.as_ptr() as *const RawInputHeader);
    if head.kind != RIM_TYPEKEYBOARD {
        return None;
    }
    let kb = *(buf.as_ptr().cast::<u8>().add(header as usize) as *const RawKeyboardData);
    let down = match kb.message {
        WM_KEYDOWN | WM_SYSKEYDOWN => true,
        WM_KEYUP | WM_SYSKEYUP => false,
        _ => return None,
    };
    let scancode = if kb.make_code == 0 {
        // Some devices (often media keys) send no scancode, only a virtual key.
        MapVirtualKeyW(kb.v_key as u32, MAPVK_VK_TO_VSC_EX) as u16
    } else {
        kb.make_code | extension(kb.flags)
    };
    let code = key_code(scancode, kb.v_key)?;
    Some((code, down))
}

fn extension(flags: u16) -> u16 {
    if flags & RI_KEY_E0 != 0 {
        0xE000
    } else if flags & RI_KEY_E1 != 0 {
        0xE100
    } else {
        0
    }
}

/// The key for an extended scancode, following winit's own raw input reading
/// so a key binds the same either way.
fn key_code(scancode: u16, v_key: u16) -> Option<KeyCode> {
    // The first half of Pause (E1 1D) and of PrtSc (E0 2A): the second half
    // names the key.
    if scancode == 0xE11D || scancode == 0xE02A {
        return None;
    }
    let key = if v_key == VK_NUMLOCK {
        // NumLock and Pause both arrive as 0x45; the virtual key tells them apart.
        PhysicalKey::Code(KeyCode::NumLock)
    } else {
        PhysicalKey::from_scancode(scancode as u32)
    };
    let PhysicalKey::Code(code) = key else {
        return None;
    };
    // With NumLock on, Shift + a numpad key is preceded by a fake Shift
    // release carrying the numpad key's scancode.
    if v_key == VK_SHIFT
        && matches!(
            code,
            KeyCode::NumpadDecimal
                | KeyCode::Numpad0
                | KeyCode::Numpad1
                | KeyCode::Numpad2
                | KeyCode::Numpad3
                | KeyCode::Numpad4
                | KeyCode::Numpad5
                | KeyCode::Numpad6
                | KeyCode::Numpad7
                | KeyCode::Numpad8
                | KeyCode::Numpad9
        )
    {
        return None;
    }
    Some(code)
}

// --- user32 / kernel32 ---------------------------------------------------

const HWND_MESSAGE: *mut c_void = -3isize as *mut c_void;
const RIDEV_INPUTSINK: u32 = 0x0000_0100;
const RID_INPUT: u32 = 0x1000_0003;
const RIM_TYPEKEYBOARD: u32 = 1;
const RI_KEY_E0: u16 = 0x02;
const RI_KEY_E1: u16 = 0x04;
const WM_INPUT: u32 = 0x00FF;
const WM_QUIT: u32 = 0x0012;
const WM_APP: u32 = 0x8000;
const PM_REMOVE: u32 = 0x0001;
const QS_ALLINPUT: u32 = 0x04FF;
const INFINITE: u32 = u32::MAX;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const VK_SHIFT: u16 = 0x10;
const VK_NUMLOCK: u16 = 0x90;
const MAPVK_VK_TO_VSC_EX: u32 = 4;
const THREAD_PRIORITY_HIGHEST: i32 = 2;

#[repr(C)]
struct RawInputDevice {
    usage_page: u16,
    usage: u16,
    flags: u32,
    target: *mut c_void,
}

#[repr(C)]
struct RawInputHeader {
    kind: u32,
    size: u32,
    device: *mut c_void,
    w_param: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RawKeyboardData {
    make_code: u16,
    flags: u16,
    reserved: u16,
    v_key: u16,
    message: u32,
    extra_information: u32,
}

#[repr(C)]
struct Msg {
    hwnd: *mut c_void,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: [i32; 2],
    // Spare room: some SDKs define a trailing private field.
    private: u32,
}

impl Default for Msg {
    fn default() -> Self {
        Self {
            hwnd: std::ptr::null_mut(),
            message: 0,
            w_param: 0,
            l_param: 0,
            time: 0,
            pt: [0; 2],
            private: 0,
        }
    }
}

#[link(name = "user32")]
extern "system" {
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: *mut c_void,
        menu: *mut c_void,
        instance: *mut c_void,
        param: *mut c_void,
    ) -> *mut c_void;
    fn RegisterRawInputDevices(devices: *const RawInputDevice, count: u32, size: u32) -> i32;
    fn GetRawInputData(
        raw_input: *mut c_void,
        command: u32,
        data: *mut c_void,
        size: *mut u32,
        header_size: u32,
    ) -> u32;
    fn PeekMessageW(msg: *mut Msg, hwnd: *mut c_void, min: u32, max: u32, remove: u32) -> i32;
    fn MsgWaitForMultipleObjects(
        count: u32,
        handles: *const *mut c_void,
        wait_all: i32,
        millis: u32,
        wake_mask: u32,
    ) -> u32;
    fn PostMessageW(hwnd: *mut c_void, msg: u32, w_param: usize, l_param: isize) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn GetForegroundWindow() -> *mut c_void;
    fn MapVirtualKeyW(code: u32, map_type: u32) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThread() -> *mut c_void;
    fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_repeat_makes_no_new_presses() {
        let mut held = Held::default();
        assert!(held.change(KeyCode::KeyZ, true));
        assert!(!held.change(KeyCode::KeyZ, true));
        assert!(!held.change(KeyCode::KeyZ, true));
        assert!(held.change(KeyCode::KeyZ, false));
        // A release of a key not held (pressed before the game was in
        // front) is not news either.
        assert!(!held.change(KeyCode::KeyX, false));
        assert!(held.change(KeyCode::KeyZ, true));
    }

    #[test]
    fn keys_held_together_come_up_one_by_one() {
        let mut held = Held::default();
        assert!(held.change(KeyCode::KeyS, true));
        assert!(held.change(KeyCode::KeyD, true));
        assert!(held.change(KeyCode::KeyS, false));
        assert!(!held.change(KeyCode::KeyS, false));
        assert!(held.change(KeyCode::KeyD, false));
    }

    #[test]
    fn scancodes_map_like_winit() {
        assert_eq!(key_code(0x2C, 0x5A), Some(KeyCode::KeyZ));
        assert_eq!(key_code(0x2A, VK_SHIFT), Some(KeyCode::ShiftLeft));
        assert_eq!(key_code(0x36, VK_SHIFT), Some(KeyCode::ShiftRight));
        assert_eq!(key_code(0x39, 0x20), Some(KeyCode::Space));
        // Extended keys carry E0.
        assert_eq!(key_code(0xE01D, 0x11), Some(KeyCode::ControlRight));
        assert_eq!(key_code(0xE04B, 0x25), Some(KeyCode::ArrowLeft));
        assert_eq!(key_code(0x4B, 0x64), Some(KeyCode::Numpad4));
        assert_eq!(0x1D | extension(RI_KEY_E0), 0xE01D);
        assert_eq!(key_code(0x45, VK_NUMLOCK), Some(KeyCode::NumLock));
        // Lead-in halves and fake shifts are dropped.
        assert_eq!(key_code(0xE11D, 0x13), None);
        assert_eq!(key_code(0xE02A, 0xFF), None);
        assert_eq!(key_code(0x4B, VK_SHIFT), None);
    }

    #[test]
    fn the_reading_thread_registers_for_raw_input() {
        // Registration needs no window in front; an empty queue to start.
        let mut keys = RawKeyboard::spawn(std::ptr::null_mut()).expect("raw input");
        assert_eq!(keys.pop(), None);
    }

    /// The next judge call within a second, skipping none.
    fn next_event(keys: &mut RawKeyboard) -> Option<LaneEvent> {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if let Some(e) = keys.pop_event() {
                return Some(e);
            }
            thread::sleep(Duration::from_millis(1));
        }
        None
    }

    #[test]
    fn a_session_checks_for_misses_until_paused_or_ended() {
        let mut keys = RawKeyboard::spawn(std::ptr::null_mut()).expect("raw input");
        assert!(keys.send(LogicCommand::Start(Box::new(LaneSession::for_test(3)))));
        let event = next_event(&mut keys).expect("a miss check");
        assert_eq!(event.session, 3);
        assert!(matches!(
            event.call,
            crate::lane_logic::JudgeCall::Misses { .. }
        ));

        for (stop, label) in [
            (LogicCommand::Pause(true), "paused"),
            (LogicCommand::End, "ended"),
        ] {
            assert!(keys.send(stop));
            // Checks made before the command was taken may still be queued.
            thread::sleep(Duration::from_millis(30));
            while keys.pop_event().is_some() {}
            thread::sleep(Duration::from_millis(30));
            assert_eq!(keys.pop_event(), None, "checks while {label}");
            assert!(keys.send(LogicCommand::Pause(false)));
        }
    }

    #[test]
    fn structs_match_the_windows_layout() {
        let ptr = std::mem::size_of::<usize>();
        assert_eq!(std::mem::size_of::<RawKeyboardData>(), 16);
        assert_eq!(std::mem::size_of::<RawInputHeader>(), 8 + 2 * ptr);
        assert_eq!(std::mem::size_of::<RawInputDevice>(), 8 + ptr);
    }
}
