//! Game controllers (HID joysticks and gamepads) read from Raw Input.
//!
//! A report arrives as `WM_INPUT` (`RIM_TYPEHID`). Its bytes are decoded
//! with hid.dll (`HidP_*`) against the device's preparsed data, which is
//! fetched once per device handle and cached by the raw input thread. The
//! result is a `PadSnapshot`: the buttons as a bitset (usage 1..=64 is bit
//! 0..=63) and up to eight axes, each normalised to 0..=65535 and kept raw
//! as well (for the turntable's wrap-aware deltas).
//!
//! Everything but the FFI is a pure function over plain values, and is
//! tested with synthetic capability tables and reports.

use std::ffi::c_void;

/// Buttons per device that are read (HID button usages 1..=64).
pub const MAX_BUTTONS: usize = 64;
/// Axes per device that are read; further value controls are ignored.
pub const MAX_AXES: usize = 8;
/// Reports larger than this are garbage and ignored.
pub const MAX_REPORT: usize = 4096;
/// Devices kept at once; others are ignored.
pub const MAX_DEVICES: usize = 16;

const PAGE_BUTTON: u16 = 0x09;
const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;
const HID_INPUT_REPORT: i32 = 0;

/// The controls of one device at one moment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PadSnapshot {
    /// Bit `i` is button usage `i + 1` (1..=64).
    pub buttons: u64,
    /// Each axis normalised to 0..=65535 over its logical range.
    pub axes: [i32; MAX_AXES],
    /// Each axis as the device sent it (unsigned, `bits` wide).
    pub raw: [u32; MAX_AXES],
}

/// Buttons that changed between two snapshots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Edges {
    pub pressed: u64,
    pub released: u64,
}

/// The button changes from `prev` to `now`, as bits.
pub fn edges(prev: u64, now: u64) -> Edges {
    Edges {
        pressed: now & !prev,
        released: prev & !now,
    }
}

/// The bit of a button usage, or `None` outside 1..=64.
pub fn usage_bit(usage: u16) -> Option<u32> {
    (1..=MAX_BUTTONS as u16)
        .contains(&usage)
        .then(|| u32::from(usage) - 1)
}

/// The change of a `bits`-wide counter from `prev` to `now`, signed and
/// wrap-aware: 250 to 3 in 8 bits is +9, 3 to 250 is -9.
pub fn wrap_delta(prev: u32, now: u32, bits: u8) -> i32 {
    let bits = u32::from(bits.clamp(1, 32));
    if bits == 32 {
        return now.wrapping_sub(prev) as i32;
    }
    let modulus = 1u64 << bits;
    let d = u64::from(now.wrapping_sub(prev)) & (modulus - 1);
    if d >= modulus / 2 {
        (d as i64 - modulus as i64) as i32
    } else {
        d as i32
    }
}

/// A logical value `raw` (`bits` wide) read as signed when the logical
/// minimum is negative.
pub fn logical_value(raw: u32, bits: u8, min: i32) -> i32 {
    let bits = u32::from(bits.clamp(1, 32));
    if min < 0 && bits < 32 && raw & (1 << (bits - 1)) != 0 {
        (i64::from(raw) - (1i64 << bits)) as i32
    } else {
        raw as i32
    }
}

/// `value` over the logical range `min..=max`, as 0..=65535. A degenerate
/// range reads as 0.
pub fn normalise(value: i32, min: i32, max: i32) -> i32 {
    if max <= min {
        return 0;
    }
    // Widened first: `max - min` and `value - min` overflow i32 over the full
    // i32 range (a panic in the input thread would abort the process).
    let (min, max) = (i64::from(min), i64::from(max));
    let value = i64::from(value).clamp(min, max);
    let scaled = (value - min) * 65535 / (max - min);
    scaled as i32
}

/// One value control of a device, as the report reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub usage_page: u16,
    pub usage: u16,
    pub bits: u8,
    pub min: i32,
    pub max: i32,
    pub axis: u8,
}

/// `HIDP_BUTTON_CAPS` (72 bytes). The union is kept as its eight words:
/// for a range they are the usage min and max, for one usage the usage is
/// the first word.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ButtonCaps {
    usage_page: u16,
    report_id: u8,
    is_alias: u8,
    bit_field: u16,
    link_collection: u16,
    link_usage: u16,
    link_usage_page: u16,
    is_range: u8,
    is_string_range: u8,
    is_designator_range: u8,
    is_absolute: u8,
    reserved: [u32; 10],
    union: [u16; 8],
}

/// `HIDP_VALUE_CAPS` (72 bytes), laid out like `ButtonCaps` plus the value
/// fields.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ValueCaps {
    usage_page: u16,
    report_id: u8,
    is_alias: u8,
    bit_field: u16,
    link_collection: u16,
    link_usage: u16,
    link_usage_page: u16,
    is_range: u8,
    is_string_range: u8,
    is_designator_range: u8,
    is_absolute: u8,
    has_null: u8,
    reserved: u8,
    bit_size: u16,
    report_count: u16,
    reserved2: [u16; 5],
    unit_exp: u32,
    units: u32,
    logical_min: i32,
    logical_max: i32,
    physical_min: i32,
    physical_max: i32,
    union: [u16; 8],
}

impl ButtonCaps {
    fn usages(&self) -> (u16, u16) {
        if self.is_range != 0 {
            (self.union[0], self.union[1])
        } else {
            (self.union[0], self.union[0])
        }
    }
}

impl ValueCaps {
    fn usages(&self) -> (u16, u16) {
        if self.is_range != 0 {
            (self.union[0], self.union[1])
        } else {
            (self.union[0], self.union[0])
        }
    }
}

/// The button usage ranges of a device (page 0x09 only), as (min, max) pairs.
pub fn button_usage_ranges(caps: &[ButtonCaps]) -> Vec<(u16, u16)> {
    caps.iter()
        .filter(|c| c.usage_page == PAGE_BUTTON)
        .map(|c| c.usages())
        .collect()
}

/// The value controls of a device, numbered as axes in capability order
/// (a range is one axis per usage). Controls past `MAX_AXES` are dropped.
pub fn value_slots(caps: &[ValueCaps]) -> Vec<Slot> {
    let mut slots = Vec::new();
    for c in caps {
        let (lo, hi) = c.usages();
        if hi < lo {
            continue;
        }
        for usage in lo..=hi {
            if slots.len() >= MAX_AXES {
                return slots;
            }
            slots.push(Slot {
                usage_page: c.usage_page,
                usage,
                bits: c.bit_size.clamp(1, 32) as u8,
                min: c.logical_min,
                max: c.logical_max,
                axis: slots.len() as u8,
            });
        }
    }
    slots
}

/// Capabilities counts of `HIDP_CAPS` (64 bytes; only the fields read here).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Caps {
    usage: u16,
    usage_page: u16,
    input_report_byte_length: u16,
    output_report_byte_length: u16,
    feature_report_byte_length: u16,
    reserved: [u16; 17],
    number_link_collection_nodes: u16,
    number_input_button_caps: u16,
    number_input_value_caps: u16,
    number_input_data_indices: u16,
    number_output_button_caps: u16,
    number_output_value_caps: u16,
    number_output_data_indices: u16,
    number_feature_button_caps: u16,
    number_feature_value_caps: u16,
    number_feature_data_indices: u16,
}

/// A controller the raw input thread can read reports from.
pub struct Device {
    /// Preparsed data as hid.dll made it (8-byte aligned storage).
    preparsed: Vec<u64>,
    report_len: usize,
    has_buttons: bool,
    slots: Vec<Slot>,
    /// Usages the buttons read into, sized once.
    usages: Vec<u16>,
    /// Snapshot of the last report, for the press / release diff.
    pub prev: PadSnapshot,
}

impl Device {
    /// Reads the device's capabilities. `None` for a device hid.dll does not
    /// describe (not a joystick or gamepad, or a broken descriptor).
    ///
    /// # Safety
    /// `handle` must be a raw input device handle from `WM_INPUT` or
    /// `WM_INPUT_DEVICE_CHANGE`.
    pub unsafe fn open(handle: *mut c_void) -> Option<Self> {
        let mut size = 0u32;
        if GetRawInputDeviceInfoW(handle, RIDI_PREPARSEDDATA, std::ptr::null_mut(), &mut size)
            == u32::MAX
            || size == 0
            || size as usize > 1 << 20
        {
            return None;
        }
        let mut preparsed = vec![0u64; (size as usize).div_ceil(8)];
        let got = GetRawInputDeviceInfoW(
            handle,
            RIDI_PREPARSEDDATA,
            preparsed.as_mut_ptr().cast(),
            &mut size,
        );
        if got == u32::MAX {
            return None;
        }
        let pre = preparsed.as_mut_ptr().cast::<c_void>();

        let mut caps = Caps::default();
        if HidP_GetCaps(pre, (&mut caps as *mut Caps).cast()) != HIDP_STATUS_SUCCESS {
            return None;
        }

        let mut buttons =
            vec![ButtonCaps::default(); usize::from(caps.number_input_button_caps.min(256))];
        let mut count = buttons.len() as u16;
        if !buttons.is_empty()
            && HidP_GetButtonCaps(
                HID_INPUT_REPORT,
                buttons.as_mut_ptr().cast(),
                &mut count,
                pre,
            ) != HIDP_STATUS_SUCCESS
        {
            return None;
        }
        buttons.truncate(usize::from(count));
        let has_buttons = !button_usage_ranges(&buttons).is_empty();

        let mut values =
            vec![ValueCaps::default(); usize::from(caps.number_input_value_caps.min(256))];
        let mut count = values.len() as u16;
        if !values.is_empty()
            && HidP_GetValueCaps(
                HID_INPUT_REPORT,
                values.as_mut_ptr().cast(),
                &mut count,
                pre,
            ) != HIDP_STATUS_SUCCESS
        {
            return None;
        }
        values.truncate(usize::from(count));
        let slots = value_slots(&values);

        Some(Self {
            preparsed,
            report_len: usize::from(caps.input_report_byte_length),
            has_buttons,
            slots,
            usages: vec![0; MAX_BUTTONS],
            prev: PadSnapshot::default(),
        })
    }

    /// The bit width of an axis, if the device has it.
    pub fn axis_bits(&self, axis: usize) -> Option<u8> {
        self.slots
            .iter()
            .find(|s| usize::from(s.axis) == axis)
            .map(|s| s.bits)
    }

    /// Decodes one report against the device's previous snapshot (`prev`, set
    /// by the caller after each report). `None` for a report that is too
    /// short, too long, or that cannot be decoded at all. Items hid.dll does
    /// not decode in this report (a report ID it does not carry, a failed
    /// buttons read) keep their previous values: see [`merge_report`].
    pub fn snapshot(&mut self, report: &[u8]) -> Option<PadSnapshot> {
        if report.len() > MAX_REPORT || report.len() < self.report_len || report.is_empty() {
            return None;
        }
        let pre = self.preparsed.as_mut_ptr().cast::<c_void>();
        let buttons: Option<usize> = if self.has_buttons {
            let mut count = self.usages.len() as u32;
            let status = unsafe {
                HidP_GetUsages(
                    HID_INPUT_REPORT,
                    PAGE_BUTTON,
                    0,
                    self.usages.as_mut_ptr(),
                    &mut count,
                    pre,
                    report.as_ptr().cast_mut(),
                    report.len() as u32,
                )
            };
            (status == HIDP_STATUS_SUCCESS).then_some(count as usize)
        } else {
            Some(0)
        };
        let usages = buttons.map(|n| &self.usages[..n.min(self.usages.len())]);
        let snap = merge_report(self.prev, &self.slots, usages, |slot| {
            let mut value = 0u32;
            let status = unsafe {
                HidP_GetUsageValue(
                    HID_INPUT_REPORT,
                    slot.usage_page,
                    0,
                    slot.usage,
                    &mut value,
                    pre,
                    report.as_ptr().cast_mut(),
                    report.len() as u32,
                )
            };
            (status == HIDP_STATUS_SUCCESS).then_some(value)
        });
        Some(snap)
    }
}

/// One report's snapshot from the previous one: the buttons are `usages`
/// when they decoded (`None` keeps the previous buttons), and each axis is
/// `read(slot)` when that decodes (`None` keeps the previous value of that
/// axis). Nothing is zeroed by a failed read, so a report of another report
/// ID leaves the axes it does not carry as they were.
pub fn merge_report(
    prev: PadSnapshot,
    slots: &[Slot],
    usages: Option<&[u16]>,
    mut read: impl FnMut(&Slot) -> Option<u32>,
) -> PadSnapshot {
    let mut snap = prev;
    if let Some(usages) = usages {
        snap.buttons = usages
            .iter()
            .filter_map(|&usage| usage_bit(usage))
            .fold(0u64, |bits, bit| bits | (1u64 << bit));
    }
    for slot in slots {
        let Some(value) = read(slot) else {
            continue;
        };
        let axis = usize::from(slot.axis);
        let bits = slot.bits;
        let raw = if bits < 32 {
            value & ((1u32 << bits) - 1)
        } else {
            value
        };
        snap.raw[axis] = raw;
        let logical = logical_value(raw, bits, slot.min);
        snap.axes[axis] = normalise(logical, slot.min, slot.max);
    }
    snap
}

// --- hid.dll / user32 / kernel32 -----------------------------------------

pub const RIDI_PREPARSEDDATA: u32 = 0x2000_0005;

#[link(name = "hid")]
extern "system" {
    fn HidP_GetCaps(preparsed: *mut c_void, caps: *mut c_void) -> i32;
    fn HidP_GetButtonCaps(
        report_type: i32,
        caps: *mut c_void,
        count: *mut u16,
        preparsed: *mut c_void,
    ) -> i32;
    fn HidP_GetValueCaps(
        report_type: i32,
        caps: *mut c_void,
        count: *mut u16,
        preparsed: *mut c_void,
    ) -> i32;
    fn HidP_GetUsages(
        report_type: i32,
        usage_page: u16,
        collection: u16,
        usage_list: *mut u16,
        usage_length: *mut u32,
        preparsed: *mut c_void,
        report: *mut u8,
        report_length: u32,
    ) -> i32;
    fn HidP_GetUsageValue(
        report_type: i32,
        usage_page: u16,
        collection: u16,
        usage: u16,
        value: *mut u32,
        preparsed: *mut c_void,
        report: *mut u8,
        report_length: u32,
    ) -> i32;
}

#[link(name = "user32")]
extern "system" {
    pub fn GetRawInputDeviceInfoW(
        device: *mut c_void,
        command: u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(page: u16, lo: u16, hi: u16, bits: u16, min: i32, max: i32, range: bool) -> ValueCaps {
        let mut c = ValueCaps::default();
        c.usage_page = page;
        c.is_range = u8::from(range);
        c.bit_size = bits;
        c.logical_min = min;
        c.logical_max = max;
        c.union[0] = lo;
        c.union[1] = hi;
        c
    }

    #[test]
    fn caps_structs_match_the_hid_layout() {
        assert_eq!(std::mem::size_of::<ButtonCaps>(), 72);
        assert_eq!(std::mem::size_of::<ValueCaps>(), 72);
        assert_eq!(std::mem::offset_of!(ValueCaps, bit_size), 18);
        assert_eq!(std::mem::offset_of!(ValueCaps, logical_min), 40);
        assert_eq!(std::mem::offset_of!(ValueCaps, union), 56);
        assert_eq!(std::mem::size_of::<Caps>(), 64);
        assert_eq!(std::mem::offset_of!(Caps, number_input_button_caps), 46);
        assert_eq!(std::mem::offset_of!(Caps, number_input_value_caps), 48);
    }

    #[test]
    fn button_usages_are_1_to_64_bits_0_to_63() {
        assert_eq!(usage_bit(0), None);
        assert_eq!(usage_bit(1), Some(0));
        assert_eq!(usage_bit(64), Some(63));
        assert_eq!(usage_bit(65), None);
        assert_eq!(usage_bit(u16::MAX), None);
    }

    #[test]
    fn button_diff_gives_presses_and_releases() {
        let prev = 0b0110u64;
        let now = 0b0011u64;
        assert_eq!(
            edges(prev, now),
            Edges {
                pressed: 0b0001,
                released: 0b0100
            }
        );
        assert_eq!(edges(now, now), Edges::default());
        // Bit 63 (button 64) takes part like any other.
        assert_eq!(edges(0, 1 << 63).pressed, 1 << 63);
    }

    #[test]
    fn wrap_delta_follows_the_short_way_round() {
        // 8-bit wrap in both directions.
        assert_eq!(wrap_delta(250, 3, 8), 9);
        assert_eq!(wrap_delta(3, 250, 8), -9);
        assert_eq!(wrap_delta(10, 14, 8), 4);
        // 16-bit wrap.
        assert_eq!(wrap_delta(65530, 4, 16), 10);
        assert_eq!(wrap_delta(4, 65530, 16), -10);
        // 10-bit and 32-bit.
        assert_eq!(wrap_delta(1020, 4, 10), 8);
        assert_eq!(wrap_delta(5, 3, 32), -2);
        // Half a turn is ambiguous: read as the positive side.
        assert_eq!(wrap_delta(0, 128, 8), -128);
        assert_eq!(wrap_delta(0, 0, 16), 0);
    }

    #[test]
    fn small_moves_are_below_the_scratch_threshold_of_two() {
        assert_eq!(wrap_delta(100, 101, 8).abs(), 1);
        assert!(wrap_delta(100, 102, 8).abs() >= 2);
    }

    #[test]
    fn normalise_maps_the_logical_range_to_0_65535() {
        assert_eq!(normalise(0, 0, 255), 0);
        assert_eq!(normalise(255, 0, 255), 65535);
        assert_eq!(normalise(128, 0, 255), 32896);
        // 10-bit range, out-of-range values clamp.
        assert_eq!(normalise(1023, 0, 1023), 65535);
        assert_eq!(normalise(2000, 0, 1023), 65535);
        // 16-bit signed range (-32768..=32767), centre near 32767.
        assert_eq!(normalise(-32768, -32768, 32767), 0);
        assert!((normalise(0, -32768, 32767) - 32768).abs() <= 1);
        // Degenerate ranges read as 0.
        assert_eq!(normalise(5, 7, 7), 0);
    }

    #[test]
    fn normalise_does_not_overflow_over_the_full_i32_range() {
        // `max - min` and `value - min` would overflow i32 here.
        assert_eq!(normalise(i32::MIN, i32::MIN, i32::MAX), 0);
        assert_eq!(normalise(i32::MAX, i32::MIN, i32::MAX), 65535);
        assert_eq!(normalise(0, i32::MIN, i32::MAX), 32767);
        assert_eq!(normalise(i32::MAX, 0, i32::MAX), 65535);
        assert_eq!(normalise(-1, i32::MIN, -1), 65535);
        assert_eq!(normalise(i32::MIN, -5, i32::MAX), 0);
    }

    #[test]
    fn negative_logical_minimum_sign_extends_the_raw_value() {
        // 8-bit signed: 0xFF is -1, 0x80 is -128.
        assert_eq!(logical_value(0xFF, 8, -128), -1);
        assert_eq!(logical_value(0x80, 8, -128), -128);
        assert_eq!(logical_value(0x7F, 8, -128), 127);
        // Unsigned range: no extension.
        assert_eq!(logical_value(0xFF, 8, 0), 255);
        assert_eq!(normalise(logical_value(0xFF, 8, -128), -128, 127), 32639);
    }

    #[test]
    fn value_controls_become_axes_in_capability_order() {
        let caps = [
            value(0x01, 0x30, 0x30, 16, 0, 65535, false), // X
            value(0x01, 0x31, 0x32, 8, 0, 255, true),     // Y and Z as a range
        ];
        let slots = value_slots(&caps);
        assert_eq!(slots.len(), 3);
        assert_eq!(
            (slots[0].usage, slots[0].axis, slots[0].bits),
            (0x30, 0, 16)
        );
        assert_eq!((slots[1].usage, slots[1].axis, slots[1].bits), (0x31, 1, 8));
        assert_eq!((slots[2].usage, slots[2].axis, slots[2].bits), (0x32, 2, 8));
    }

    #[test]
    fn value_controls_stop_at_eight_axes_and_garbage_ranges_are_skipped() {
        let mut caps = vec![value(0x01, 0x30, 0x30 + 19, 8, 0, 255, true)];
        // An inverted range is ignored; an oversized one is capped at 8.
        caps.push(value(0x01, 9, 2, 8, 0, 255, true));
        let slots = value_slots(&caps);
        assert_eq!(slots.len(), MAX_AXES);
        assert!(slots.iter().all(|s| s.usage >= 0x30));
    }

    #[test]
    fn button_ranges_keep_only_the_button_page() {
        let mut b = ButtonCaps::default();
        b.usage_page = PAGE_BUTTON;
        b.is_range = 1;
        b.union[0] = 1;
        b.union[1] = 32;
        let mut other = b;
        other.usage_page = 0x0C;
        assert_eq!(button_usage_ranges(&[b, other]), vec![(1, 32)]);
    }

    fn axis_slot(axis: u8, bits: u8) -> Slot {
        Slot {
            usage_page: 0x01,
            usage: 0x30 + u16::from(axis),
            bits,
            min: 0,
            max: (1 << bits) - 1,
            axis,
        }
    }

    #[test]
    fn a_report_of_another_report_id_keeps_the_axes_it_does_not_carry() {
        // Two report IDs: report 1 carries buttons only, report 2 the axis.
        // The axis read fails for report 1 (hid.dll: incompatible report ID).
        let slots = [axis_slot(0, 8)];
        let prev = merge_report(PadSnapshot::default(), &slots, Some(&[1]), |_| Some(200));
        assert_eq!(prev.raw[0], 200);

        let after_report_1 = merge_report(prev, &slots, Some(&[1, 2]), |_| None);
        assert_eq!(after_report_1.raw[0], 200, "axis must not be zeroed");
        assert_eq!(after_report_1.axes[0], prev.axes[0]);
        assert_eq!(after_report_1.buttons, 0b11);
        // The next report 2 moves by 3 from the kept value, not from zero.
        let moved = merge_report(after_report_1, &slots, Some(&[1, 2]), |_| Some(203));
        assert_eq!(wrap_delta(after_report_1.raw[0], moved.raw[0], 8), 3);
    }

    #[test]
    fn a_failed_buttons_decode_keeps_the_buttons_and_the_axes() {
        let slots = [axis_slot(0, 16)];
        let prev = PadSnapshot {
            buttons: 0b101,
            axes: [0; MAX_AXES],
            raw: {
                let mut raw = [0u32; MAX_AXES];
                raw[0] = 4000;
                raw
            },
        };
        let now = merge_report(prev, &slots, None, |_| Some(4010));
        assert_eq!(now.buttons, 0b101, "no button edges from a failed read");
        assert_eq!(now.raw[0], 4010, "the axis still decodes");
        assert_eq!(edges(prev.buttons, now.buttons), Edges::default());
    }

    #[test]
    fn decoded_buttons_replace_the_previous_set() {
        let prev = PadSnapshot {
            buttons: 0b1111,
            ..PadSnapshot::default()
        };
        // Usages 2 and 64 are read; 0 and 65 are outside the button range.
        let now = merge_report(prev, &[], Some(&[2, 64, 0, 65]), |_| None);
        assert_eq!(now.buttons, (1 << 1) | (1 << 63));
    }

    #[test]
    fn a_failed_axis_read_leaves_only_that_axis() {
        let slots = [axis_slot(0, 8), axis_slot(1, 8)];
        let prev = merge_report(PadSnapshot::default(), &slots, Some(&[]), |s| {
            Some(if s.axis == 0 { 10 } else { 20 })
        });
        let now = merge_report(prev, &slots, Some(&[]), |s| (s.axis == 1).then_some(25));
        assert_eq!(now.raw, [10, 25, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn axes_normalise_per_device_range_and_keep_raw() {
        // Without hardware: a 10-bit axis on a 0..1023 range and a value that
        // the wrap delta sees across the top of its range.
        let slots = value_slots(&[value(0x01, 0x30, 0x30, 10, 0, 1023, false)]);
        assert_eq!(slots[0].bits, 10);
        let raw_prev = 1020;
        let raw_now = 4;
        assert_eq!(wrap_delta(raw_prev, raw_now, slots[0].bits), 8);
        assert_eq!(normalise(i32::try_from(raw_now).unwrap(), 0, 1023), 256);
    }
}
