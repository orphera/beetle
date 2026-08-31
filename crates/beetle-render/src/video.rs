//! Video BGA decoding and playback engine.
//! Supports MP4, WMV, AVI, MPG, and MOV video formats using OS-native Windows Media Foundation
//! when the `bga-enhanced` feature is enabled on Windows, with a zero-overhead stub fallback.

use std::path::Path;
use crate::image::ImageBuffer;

/// Standard video file extensions supported by the BGA video player.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mpg", "mpeg", "wmv", "avi", "mov", "webm", "mkv",
];

/// Checks if a file path has a recognized video extension.
pub fn is_video_path<P: AsRef<Path>>(path: P) -> bool {
    if let Some(ext) = path.as_ref().extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        VIDEO_EXTENSIONS.iter().any(|&ve| ve == ext_lower)
    } else {
        false
    }
}

#[cfg(all(feature = "bga-enhanced", target_os = "windows"))]
mod wmf_backend {
    use super::*;
    use crate::skin::ColorRgba;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    #[repr(C)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct GUID {
        pub data1: u32,
        pub data2: u16,
        pub data3: u16,
        pub data4: [u8; 8],
    }

    const MF_VERSION: u32 = 0x0002;
    const MFSTARTUP_NOSOCKET: u32 = 0x1;
    const MF_SOURCE_READER_FIRST_VIDEO_STREAM: u32 = 0xFFFFFFFC;
    const MF_SOURCE_READERF_ENDOFSTREAM: u32 = 0x00000200;

    const MF_MT_MAJOR_TYPE: GUID = GUID {
        data1: 0x48eba18e,
        data2: 0xf827,
        data3: 0x49ed,
        data4: [0x85, 0x5d, 0x25, 0x92, 0x4b, 0x77, 0x82, 0x52],
    };

    #[allow(non_upper_case_globals)]
    const MFMediaType_Video: GUID = GUID {
        data1: 0x73646976,
        data2: 0x0000,
        data3: 0x0010,
        data4: [0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71],
    };

    const MF_MT_SUBTYPE: GUID = GUID {
        data1: 0xf7e34c9a,
        data2: 0x42e8,
        data3: 0x4714,
        data4: [0xb7, 0x4b, 0xcb, 0x29, 0xd7, 0x2c, 0x35, 0xe5],
    };

    #[allow(non_upper_case_globals)]
    const MFVideoFormat_RGB32: GUID = GUID {
        data1: 0x00000016,
        data2: 0x0000,
        data3: 0x0010,
        data4: [0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71],
    };

    const MF_MT_FRAME_SIZE: GUID = GUID {
        data1: 0x1652c33d,
        data2: 0xd6b2,
        data3: 0x4012,
        data4: [0xb8, 0x34, 0x72, 0x03, 0x08, 0x49, 0xa3, 0x7d],
    };

    #[repr(C)]
    #[allow(non_snake_case, dead_code)]
    struct PROPVARIANT {
        vt: u16,
        w_reserved1: u16,
        w_reserved2: u16,
        w_reserved3: u16,
        val: i64,
    }

    #[repr(C)]
    #[allow(non_snake_case, dead_code)]
    struct IUnknownVtbl {
        QueryInterface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32,
        AddRef: unsafe extern "system" fn(*mut c_void) -> u32,
        Release: unsafe extern "system" fn(*mut c_void) -> u32,
    }

    #[repr(C)]
    #[allow(non_snake_case, dead_code)]
    struct IMFMediaTypeVtbl {
        parent: IUnknownVtbl,
        _unused1: [*const c_void; 5],
        GetUINT64: unsafe extern "system" fn(*mut c_void, *const GUID, *mut u64) -> i32,
        _unused2: [*const c_void; 15],
        SetGUID: unsafe extern "system" fn(*mut c_void, *const GUID, *const GUID) -> i32,
        _unused3: [*const c_void; 13],
    }

    #[repr(C)]
    #[allow(non_snake_case, dead_code)]
    struct IMFSourceReaderVtbl {
        parent: IUnknownVtbl,
        _unused1: [*const c_void; 3],
        GetCurrentMediaType: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
        SetCurrentMediaType: unsafe extern "system" fn(*mut c_void, u32, *mut u32, *mut c_void) -> i32,
        SetCurrentPosition: unsafe extern "system" fn(*mut c_void, *const GUID, *const PROPVARIANT) -> i32,
        ReadSample: unsafe extern "system" fn(
            *mut c_void,
            u32,
            u32,
            *mut u32,
            *mut u32,
            *mut i64,
            *mut *mut c_void,
        ) -> i32,
        _unused2: [*const c_void; 3],
    }

    #[repr(C)]
    #[allow(non_snake_case, dead_code)]
    struct IMFSampleVtbl {
        parent: IUnknownVtbl,
        unused_attributes: [*const c_void; 29],
        GetSampleFlags: *const c_void,
        SetSampleFlags: *const c_void,
        GetSampleTime: unsafe extern "system" fn(*mut c_void, *mut i64) -> i32,
        SetSampleTime: *const c_void,
        GetSampleDuration: unsafe extern "system" fn(*mut c_void, *mut i64) -> i32,
        SetSampleDuration: *const c_void,
        GetBufferCount: *const c_void,
        GetBufferByIndex: *const c_void,
        ConvertToContiguousBuffer: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    }

    #[repr(C)]
    #[allow(non_snake_case, dead_code)]
    struct IMFMediaBufferVtbl {
        parent: IUnknownVtbl,
        Lock: unsafe extern "system" fn(*mut c_void, *mut *mut u8, *mut u32, *mut u32) -> i32,
        Unlock: unsafe extern "system" fn(*mut c_void) -> i32,
        GetCurrentLength: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32,
        _unused: [*const c_void; 2],
    }

    #[link(name = "mfplat")]
    #[link(name = "mfreadwrite")]
    #[link(name = "ole32")]
    extern "system" {
        fn MFStartup(version: u32, dwFlags: u32) -> i32;
        fn MFShutdown() -> i32;
        fn MFCreateSourceReaderFromURL(
            pwszURL: *const u16,
            pAttributes: *mut c_void,
            ppSourceReader: *mut *mut c_void,
        ) -> i32;
        fn MFCreateMediaType(ppMFType: *mut *mut c_void) -> i32;
        fn CoInitializeEx(pvReserved: *mut c_void, dwCoInit: u32) -> i32;
        fn CoUninitialize();
    }

    pub struct WmfVideoPlayer {
        reader: *mut c_void,
        pub width: u32,
        pub height: u32,
        pub current_time_seconds: f64,
        pub frame_buffer: ImageBuffer,
        pub is_eof: bool,
    }

    impl WmfVideoPlayer {
        pub fn open(path: &Path) -> Option<Self> {
            unsafe {
                let _ = CoInitializeEx(ptr::null_mut(), 0x0); // COINIT_MULTITHREADED
                let _ = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET);

                let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
                let mut reader_ptr: *mut c_void = ptr::null_mut();

                let hr = MFCreateSourceReaderFromURL(wide_path.as_ptr(), ptr::null_mut(), &mut reader_ptr);
                if hr < 0 || reader_ptr.is_null() {
                    return None;
                }

                let reader_vtbl = *(reader_ptr as *mut *mut IMFSourceReaderVtbl);

                // Create requested media type (RGB32)
                let mut media_type_ptr: *mut c_void = ptr::null_mut();
                if MFCreateMediaType(&mut media_type_ptr) < 0 || media_type_ptr.is_null() {
                    let _ = ((*reader_vtbl).parent.Release)(reader_ptr);
                    let _ = MFShutdown();
                    CoUninitialize();
                    return None;
                }

                let mt_vtbl = *(media_type_ptr as *mut *mut IMFMediaTypeVtbl);
                let _ = ((*mt_vtbl).SetGUID)(media_type_ptr, &MF_MT_MAJOR_TYPE, &MFMediaType_Video);
                let _ = ((*mt_vtbl).SetGUID)(media_type_ptr, &MF_MT_SUBTYPE, &MFVideoFormat_RGB32);

                let hr = ((*reader_vtbl).SetCurrentMediaType)(
                    reader_ptr,
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM,
                    ptr::null_mut(),
                    media_type_ptr,
                );
                let _ = ((*mt_vtbl).parent.Release)(media_type_ptr);

                if hr < 0 {
                    let _ = ((*reader_vtbl).parent.Release)(reader_ptr);
                    let _ = MFShutdown();
                    CoUninitialize();
                    return None;
                }

                // Query video dimensions
                let mut current_mt: *mut c_void = ptr::null_mut();
                let mut width = 320;
                let mut height = 240;

                if ((*reader_vtbl).GetCurrentMediaType)(
                    reader_ptr,
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM,
                    &mut current_mt,
                ) >= 0 && !current_mt.is_null()
                {
                    let cur_vtbl = *(current_mt as *mut *mut IMFMediaTypeVtbl);
                    let mut frame_size: u64 = 0;
                    if ((*cur_vtbl).GetUINT64)(current_mt, &MF_MT_FRAME_SIZE, &mut frame_size) >= 0 {
                        let w = (frame_size >> 32) as u32;
                        let h = (frame_size & 0xFFFFFFFF) as u32;
                        if w > 0 && h > 0 {
                            width = w;
                            height = h;
                        }
                    }
                    let _ = ((*cur_vtbl).parent.Release)(current_mt);
                }

                let mut player = Self {
                    reader: reader_ptr,
                    width,
                    height,
                    current_time_seconds: -1.0,
                    frame_buffer: ImageBuffer::new(width, height, ColorRgba::new(0, 0, 0, 255)),
                    is_eof: false,
                };

                // Read initial frame
                player.read_next_frame();
                Some(player)
            }
        }

        pub fn seek(&mut self, time_seconds: f64) {
            unsafe {
                if self.reader.is_null() {
                    return;
                }
                let reader_vtbl = *(self.reader as *mut *mut IMFSourceReaderVtbl);
                let time_100ns = (time_seconds.max(0.0) * 10_000_000.0) as i64;
                let var = PROPVARIANT {
                    vt: 20, // VT_I8
                    w_reserved1: 0,
                    w_reserved2: 0,
                    w_reserved3: 0,
                    val: time_100ns,
                };
                let guid_null = GUID { data1: 0, data2: 0, data3: 0, data4: [0; 8] };
                let _ = ((*reader_vtbl).SetCurrentPosition)(self.reader, &guid_null, &var);
                self.is_eof = false;
                self.current_time_seconds = time_seconds;
            }
        }

        pub fn read_next_frame(&mut self) -> bool {
            unsafe {
                if self.reader.is_null() || self.is_eof {
                    return false;
                }

                let reader_vtbl = *(self.reader as *mut *mut IMFSourceReaderVtbl);
                let mut actual_stream_index = 0u32;
                let mut stream_flags = 0u32;
                let mut timestamp_100ns = 0i64;
                let mut sample_ptr: *mut c_void = ptr::null_mut();

                let hr = ((*reader_vtbl).ReadSample)(
                    self.reader,
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM,
                    0,
                    &mut actual_stream_index,
                    &mut stream_flags,
                    &mut timestamp_100ns,
                    &mut sample_ptr,
                );

                if hr < 0 || (stream_flags & MF_SOURCE_READERF_ENDOFSTREAM) != 0 {
                    self.is_eof = true;
                    if !sample_ptr.is_null() {
                        let sample_vtbl = *(sample_ptr as *mut *mut IMFSampleVtbl);
                        let _ = ((*sample_vtbl).parent.Release)(sample_ptr);
                    }
                    return false;
                }

                if sample_ptr.is_null() {
                    return false;
                }

                let sample_vtbl = *(sample_ptr as *mut *mut IMFSampleVtbl);
                let mut buffer_ptr: *mut c_void = ptr::null_mut();

                let hr = ((*sample_vtbl).ConvertToContiguousBuffer)(sample_ptr, &mut buffer_ptr);
                if hr >= 0 && !buffer_ptr.is_null() {
                    let buf_vtbl = *(buffer_ptr as *mut *mut IMFMediaBufferVtbl);
                    let mut data_ptr: *mut u8 = ptr::null_mut();
                    let mut max_len = 0u32;
                    let mut cur_len = 0u32;

                    if ((*buf_vtbl).Lock)(buffer_ptr, &mut data_ptr, &mut max_len, &mut cur_len) >= 0
                        && !data_ptr.is_null()
                    {
                        let available = cur_len as usize;
                        let row_bytes = (self.width * 4) as usize;
                        let stride = if self.height > 0 { available / self.height as usize } else { row_bytes };

                        if stride >= row_bytes && available >= row_bytes * self.height as usize {
                            let src_slice = std::slice::from_raw_parts(data_ptr, available);
                            for row in 0..self.height as usize {
                                let row_start = row * stride;
                                if row_start + row_bytes <= available {
                                    let row_slice = &src_slice[row_start..row_start + row_bytes];
                                    let dest_row_start = row * self.width as usize;
                                    for (col, chunk) in row_slice.chunks_exact(4).enumerate() {
                                        let idx = dest_row_start + col;
                                        if idx < self.frame_buffer.pixels.len() {
                                            self.frame_buffer.pixels[idx] =
                                                ColorRgba::new(chunk[2], chunk[1], chunk[0], 255);
                                        }
                                    }
                                }
                            }
                        }

                        let _ = ((*buf_vtbl).Unlock)(buffer_ptr);
                    }
                    let _ = ((*buf_vtbl).parent.Release)(buffer_ptr);
                }

                let _ = ((*sample_vtbl).parent.Release)(sample_ptr);
                self.current_time_seconds = timestamp_100ns as f64 / 10_000_000.0;
                true
            }
        }

        pub fn update(&mut self, audio_time_seconds: f64) -> Option<&ImageBuffer> {
            // Instant seek if timing drifted, rewound, or started far ahead (e.g. practice mode fast forward)
            if audio_time_seconds < self.current_time_seconds - 0.5
                || (self.current_time_seconds >= 0.0 && audio_time_seconds > self.current_time_seconds + 1.0)
                || (self.current_time_seconds < 0.0 && audio_time_seconds > 1.0)
            {
                self.seek(audio_time_seconds);
            }

            // Catch up to current audio time without blocking UI (limit catch-up to max 5 frames per render frame)
            let mut frames_read = 0;
            while !self.is_eof && self.current_time_seconds < audio_time_seconds && frames_read < 5 {
                if !self.read_next_frame() {
                    break;
                }
                frames_read += 1;
            }

            Some(&self.frame_buffer)
        }
    }

    impl Drop for WmfVideoPlayer {
        fn drop(&mut self) {
            unsafe {
                if !self.reader.is_null() {
                    let reader_vtbl = *(self.reader as *mut *mut IMFSourceReaderVtbl);
                    let _ = ((*reader_vtbl).parent.Release)(self.reader);
                    self.reader = ptr::null_mut();
                }
                let _ = MFShutdown();
                CoUninitialize();
            }
        }
    }
}

/// Unified BGA video player interface.
pub struct BgaVideoPlayer {
    #[cfg(all(feature = "bga-enhanced", target_os = "windows"))]
    backend: Option<wmf_backend::WmfVideoPlayer>,
    #[cfg(not(all(feature = "bga-enhanced", target_os = "windows")))]
    _dummy: (),
}

impl BgaVideoPlayer {
    /// Opens a video file from disk if supported and feature is active.
    pub fn open<P: AsRef<Path>>(path: P) -> Option<Self> {
        let p = path.as_ref();
        if !is_video_path(p) {
            return None;
        }

        #[cfg(all(feature = "bga-enhanced", target_os = "windows"))]
        {
            let backend = wmf_backend::WmfVideoPlayer::open(p)?;
            Some(Self { backend: Some(backend) })
        }

        #[cfg(not(all(feature = "bga-enhanced", target_os = "windows")))]
        {
            None
        }
    }

    /// Advances video playback synchronized to `audio_time_seconds` and returns the current frame.
    pub fn update(&mut self, audio_time_seconds: f64) -> Option<&ImageBuffer> {
        #[cfg(all(feature = "bga-enhanced", target_os = "windows"))]
        {
            self.backend.as_mut().and_then(|b| b.update(audio_time_seconds))
        }

        #[cfg(not(all(feature = "bga-enhanced", target_os = "windows")))]
        {
            let _ = audio_time_seconds;
            None
        }
    }

    /// Returns the current decoded frame buffer if available.
    pub fn current_frame(&self) -> Option<&ImageBuffer> {
        #[cfg(all(feature = "bga-enhanced", target_os = "windows"))]
        {
            self.backend.as_ref().map(|b| &b.frame_buffer)
        }

        #[cfg(not(all(feature = "bga-enhanced", target_os = "windows")))]
        {
            None
        }
    }

    /// Returns video width.
    pub fn width(&self) -> u32 {
        #[cfg(all(feature = "bga-enhanced", target_os = "windows"))]
        {
            self.backend.as_ref().map(|b| b.width).unwrap_or(0)
        }

        #[cfg(not(all(feature = "bga-enhanced", target_os = "windows")))]
        {
            0
        }
    }

    /// Returns video height.
    pub fn height(&self) -> u32 {
        #[cfg(all(feature = "bga-enhanced", target_os = "windows"))]
        {
            self.backend.as_ref().map(|b| b.height).unwrap_or(0)
        }

        #[cfg(not(all(feature = "bga-enhanced", target_os = "windows")))]
        {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_video_path() {
        assert!(is_video_path("bga.mp4"));
        assert!(is_video_path("video.WMV"));
        assert!(is_video_path("folder/movie.avi"));
        assert!(is_video_path("bg.mpg"));
        assert!(is_video_path("movie.webm"));
        assert!(!is_video_path("image.png"));
        assert!(!is_video_path("stage.bmp"));
        assert!(!is_video_path("audio.wav"));
        assert!(!is_video_path("chart.bms"));
    }

    #[test]
    fn test_bga_video_player_nonexistent() {
        let player = BgaVideoPlayer::open("nonexistent_video_file.mp4");
        assert!(player.is_none());
    }
}
