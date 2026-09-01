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
        data2: 0xf8c9,
        data3: 0x4687,
        data4: [0xbf, 0x11, 0x0a, 0x74, 0xc9, 0xf9, 0x6a, 0x8f],
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

    const MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING: GUID = GUID {
        data1: 0xfb394f3d,
        data2: 0xccf1,
        data3: 0x42ee,
        data4: [0xbb, 0xb3, 0xf9, 0xb8, 0x45, 0xd5, 0x68, 0x1d],
    };

    const MF_BYTESTREAM_ORIGIN_NAME: GUID = GUID {
        data1: 0xfc358288,
        data2: 0x3cb6,
        data3: 0x460c,
        data4: [0xa4, 0x24, 0xb6, 0x68, 0x12, 0x60, 0x37, 0x5a],
    };

    #[allow(non_upper_case_globals)]
    const IID_IMFAttributes: GUID = GUID {
        data1: 0x2cd2d921,
        data2: 0xc447,
        data3: 0x44a7,
        data4: [0xa1, 0x3c, 0x4a, 0xda, 0xbf, 0xc2, 0x47, 0xe3],
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
    struct IMFAttributesVtbl {
        parent: IUnknownVtbl,
        _unused1: [*const c_void; 18],
        SetUINT32: unsafe extern "system" fn(*mut c_void, *const GUID, u32) -> i32,
        _unused2: [*const c_void; 3],
        SetString: unsafe extern "system" fn(*mut c_void, *const GUID, *const u16) -> i32,
        _unused3: [*const c_void; 7],
    }

    #[repr(C)]
    #[allow(non_snake_case, dead_code)]
    struct IMFMediaTypeVtbl {
        parent: IUnknownVtbl,
        _unused1: [*const c_void; 5],
        GetUINT64: unsafe extern "system" fn(*mut c_void, *const GUID, *mut u64) -> i32,
        _unused2: [*const c_void; 15],
        SetGUID: unsafe extern "system" fn(*mut c_void, *const GUID, *const GUID) -> i32,
        _unused3: [*const c_void; 14],
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
        unused_attributes: [*const c_void; 30],
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
    #[link(name = "shlwapi")]
    extern "system" {
        fn MFStartup(version: u32, dwFlags: u32) -> i32;
        fn MFShutdown() -> i32;
        fn MFCreateSourceReaderFromURL(
            pwszURL: *const u16,
            pAttributes: *mut c_void,
            ppSourceReader: *mut *mut c_void,
        ) -> i32;
        fn MFCreateSourceReaderFromByteStream(
            pByteStream: *mut c_void,
            pAttributes: *mut c_void,
            ppSourceReader: *mut *mut c_void,
        ) -> i32;
        fn MFCreateMFByteStreamOnStream(
            pStream: *mut c_void,
            ppByteStream: *mut *mut c_void,
        ) -> i32;
        fn SHCreateMemStream(pInit: *const u8, cbInit: u32) -> *mut c_void;
        fn MFCreateMediaType(ppMFType: *mut *mut c_void) -> i32;
        fn MFCreateAttributes(ppMFAttributes: *mut *mut c_void, cInitialSize: u32) -> i32;
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
        unsafe fn init_with_reader(reader_ptr: *mut c_void) -> Option<Self> {
            let reader_vtbl = *(reader_ptr as *mut *mut IMFSourceReaderVtbl);

            // Create requested media type (RGB32)
            let mut media_type_ptr: *mut c_void = ptr::null_mut();
            let hr_mt = MFCreateMediaType(&mut media_type_ptr);
            if hr_mt < 0 || media_type_ptr.is_null() {
                eprintln!("[WMF] MFCreateMediaType failed: hr=0x{:08X}", hr_mt as u32);
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
                eprintln!("[WMF] SetCurrentMediaType (RGB32) failed: hr=0x{:08X}", hr as u32);
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

        pub fn open(path: &Path) -> Option<Self> {
            unsafe {
                let _ = CoInitializeEx(ptr::null_mut(), 0x0); // COINIT_MULTITHREADED
                let _ = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET);

                let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
                let mut reader_ptr: *mut c_void = ptr::null_mut();

                // Enable video processing (YUV -> RGB32 conversion) via IMFAttributes
                let mut attr_ptr: *mut c_void = ptr::null_mut();
                let hr_attr = MFCreateAttributes(&mut attr_ptr, 1);
                if hr_attr >= 0 && !attr_ptr.is_null() {
                    let attr_vtbl = *(attr_ptr as *mut *mut IMFAttributesVtbl);
                    let _ = ((*attr_vtbl).SetUINT32)(
                        attr_ptr,
                        &MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
                        1,
                    );
                }

                let hr = MFCreateSourceReaderFromURL(wide_path.as_ptr(), attr_ptr, &mut reader_ptr);

                if !attr_ptr.is_null() {
                    let attr_vtbl = *(attr_ptr as *mut *mut IMFAttributesVtbl);
                    let _ = ((*attr_vtbl).parent.Release)(attr_ptr);
                }

                if hr < 0 || reader_ptr.is_null() {
                    eprintln!("[WMF] MFCreateSourceReaderFromURL failed: hr=0x{:08X}", hr as u32);
                    let _ = MFShutdown();
                    CoUninitialize();
                    return None;
                }

                Self::init_with_reader(reader_ptr)
            }
        }

        pub fn open_from_memory(bytes: &[u8], filename_hint: Option<&str>) -> Option<Self> {
            unsafe {
                let _ = CoInitializeEx(ptr::null_mut(), 0x0); // COINIT_MULTITHREADED
                let _ = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET);

                if bytes.is_empty() {
                    let _ = MFShutdown();
                    CoUninitialize();
                    return None;
                }

                // 1. Create IStream on memory buffer
                let stream_ptr = SHCreateMemStream(bytes.as_ptr(), bytes.len() as u32);
                if stream_ptr.is_null() {
                    eprintln!("[WMF] SHCreateMemStream failed");
                    let _ = MFShutdown();
                    CoUninitialize();
                    return None;
                }

                // 2. Wrap IStream in IMFByteStream
                let mut byte_stream_ptr: *mut c_void = ptr::null_mut();
                let hr_bs = MFCreateMFByteStreamOnStream(stream_ptr, &mut byte_stream_ptr);
                let stream_vtbl = *(stream_ptr as *mut *mut IUnknownVtbl);
                let _ = ((*stream_vtbl).Release)(stream_ptr);

                if hr_bs < 0 || byte_stream_ptr.is_null() {
                    eprintln!("[WMF] MFCreateMFByteStreamOnStream failed: hr=0x{:08X}", hr_bs as u32);
                    let _ = MFShutdown();
                    CoUninitialize();
                    return None;
                }

                // If a filename hint was provided, set MF_BYTESTREAM_ORIGIN_NAME on the byte stream
                if let Some(hint) = filename_hint {
                    let bs_vtbl = *(byte_stream_ptr as *mut *mut IUnknownVtbl);
                    let mut attr_obj: *mut c_void = ptr::null_mut();
                    let hr_qi = ((*bs_vtbl).QueryInterface)(byte_stream_ptr, &IID_IMFAttributes, &mut attr_obj);
                    if hr_qi >= 0 && !attr_obj.is_null() {
                        let attr_vtbl = *(attr_obj as *mut *mut IMFAttributesVtbl);
                        let wide_hint: Vec<u16> = hint.encode_utf16().chain(Some(0)).collect();
                        let _ = ((*attr_vtbl).SetString)(attr_obj, &MF_BYTESTREAM_ORIGIN_NAME, wide_hint.as_ptr());
                        let _ = ((*attr_vtbl).parent.Release)(attr_obj);
                    }
                }

                // 3. Create SourceReader with video processing attributes
                let mut attr_ptr: *mut c_void = ptr::null_mut();
                let hr_attr = MFCreateAttributes(&mut attr_ptr, 1);
                if hr_attr >= 0 && !attr_ptr.is_null() {
                    let attr_vtbl = *(attr_ptr as *mut *mut IMFAttributesVtbl);
                    let _ = ((*attr_vtbl).SetUINT32)(
                        attr_ptr,
                        &MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
                        1,
                    );
                }

                let mut reader_ptr: *mut c_void = ptr::null_mut();
                let hr = MFCreateSourceReaderFromByteStream(byte_stream_ptr, attr_ptr, &mut reader_ptr);

                if !attr_ptr.is_null() {
                    let attr_vtbl = *(attr_ptr as *mut *mut IMFAttributesVtbl);
                    let _ = ((*attr_vtbl).parent.Release)(attr_ptr);
                }

                let bs_vtbl = *(byte_stream_ptr as *mut *mut IUnknownVtbl);
                let _ = ((*bs_vtbl).Release)(byte_stream_ptr);

                if hr < 0 || reader_ptr.is_null() {
                    eprintln!("[WMF] MFCreateSourceReaderFromByteStream failed: hr=0x{:08X}", hr as u32);
                    let _ = MFShutdown();
                    CoUninitialize();
                    return None;
                }

                Self::init_with_reader(reader_ptr)
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

                if hr < 0 {
                    eprintln!("[WMF] ReadSample failed: hr=0x{:08X}", hr as u32);
                    self.is_eof = true;
                    if !sample_ptr.is_null() {
                        let sample_vtbl = *(sample_ptr as *mut *mut IMFSampleVtbl);
                        let _ = ((*sample_vtbl).parent.Release)(sample_ptr);
                    }
                    return false;
                }

                if (stream_flags & MF_SOURCE_READERF_ENDOFSTREAM) != 0 {
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
                if hr < 0 || buffer_ptr.is_null() {
                    eprintln!("[WMF] ConvertToContiguousBuffer failed: hr=0x{:08X}", hr as u32);
                } else {
                    let buf_vtbl = *(buffer_ptr as *mut *mut IMFMediaBufferVtbl);
                    let mut data_ptr: *mut u8 = ptr::null_mut();
                    let mut max_len = 0u32;
                    let mut cur_len = 0u32;

                    let hr_lock = ((*buf_vtbl).Lock)(buffer_ptr, &mut data_ptr, &mut max_len, &mut cur_len);
                    if hr_lock < 0 || data_ptr.is_null() {
                        eprintln!("[WMF] Buffer Lock failed: hr=0x{:08X}", hr_lock as u32);
                    } else {
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

    /// Opens a video directly from an in-memory byte buffer (e.g. from a .bmsp archive)
    /// without extracting anything to disk.
    pub fn open_from_memory(bytes: &[u8], filename_hint: Option<&str>) -> Option<Self> {
        if let Some(hint) = filename_hint {
            if !is_video_path(hint) {
                return None;
            }
        }

        #[cfg(all(feature = "bga-enhanced", target_os = "windows"))]
        {
            let backend = wmf_backend::WmfVideoPlayer::open_from_memory(bytes, filename_hint)?;
            Some(Self { backend: Some(backend) })
        }

        #[cfg(not(all(feature = "bga-enhanced", target_os = "windows")))]
        {
            let _ = (bytes, filename_hint);
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

    #[test]
    fn test_wmf_probe_real_video() {
        let sample_path = "sample_640x480.mp4";
        if std::path::Path::new(sample_path).exists() {
            let player = BgaVideoPlayer::open(sample_path).expect("Failed to open sample_640x480.mp4");
            assert_eq!(player.width(), 640);
            assert_eq!(player.height(), 480);
            assert!(player.current_frame().is_some());

            let bytes = std::fs::read(sample_path).unwrap();
            let mem_player = BgaVideoPlayer::open_from_memory(&bytes, Some("sample_640x480.mp4")).expect("Failed to open from memory");
            assert_eq!(mem_player.width(), 640);
            assert_eq!(mem_player.height(), 480);
            assert!(mem_player.current_frame().is_some());
        }

        let test_paths = [
            "../../../bms/roop_dotm_ogg/bga.mp4",
            "../../../bms/roop_dotm_ogg/bga.wmv",
            "../../../bms/[TJhangneil]ozma/ozma.mpg",
        ];
        for p in &test_paths {
            let path = std::path::Path::new(p);
            if path.exists() {
                let player = BgaVideoPlayer::open(path);
                if let Some(pl) = player {
                    assert!(pl.current_frame().is_some());
                }

                if let Ok(bytes) = std::fs::read(path) {
                    let filename = path.file_name().and_then(|n| n.to_str());
                    let mem_player = BgaVideoPlayer::open_from_memory(&bytes, filename);
                    if let Some(mpl) = mem_player {
                        assert!(mpl.current_frame().is_some());
                    }
                }
            }
        }
    }
}
