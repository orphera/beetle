//! BGA playback of MPEG-1/2 video with the built-in decoder (`mpeg-video`),
//! so `.mpg` movies play without Windows' optional MPEG-2 extension.
//!
//! A worker thread decodes ahead into a short queue of RGBA frames; the
//! render thread only takes the frame due at the audio time. A jump back in
//! time (or far ahead) restarts the worker at the frame needed.

use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;

use mpeg_video::{ps, Decoder};

use crate::image::ImageBuffer;
use crate::skin::ColorRgba;

/// Frames decoded ahead of the one shown.
const QUEUE: usize = 4;

pub struct MpegPlayer {
    stream: Arc<Vec<u8>>,
    frames: Receiver<(usize, ImageBuffer)>,
    fps: f64,
    pub width: u32,
    pub height: u32,
    /// The frame shown, and its index (`usize::MAX` before the first).
    pub frame_buffer: ImageBuffer,
    shown: usize,
    pub frame_serial: u64,
    ended: bool,
}

/// Whether `bytes` are an MPEG program or video stream this player takes.
pub fn is_mpeg(bytes: &[u8]) -> bool {
    ps::is_mpeg(bytes)
}

/// Decodes from frame 0, sending frames from `from` on, until the receiver goes.
fn spawn(stream: Arc<Vec<u8>>, from: usize) -> Receiver<(usize, ImageBuffer)> {
    let (tx, rx) = sync_channel(QUEUE);
    thread::spawn(move || {
        let mut decoder = Decoder::new(stream.to_vec());
        let mut index = 0usize;
        while let Some(Ok(frame)) = decoder.next_frame() {
            if index >= from {
                let mut image =
                    ImageBuffer::new(frame.width, frame.height, ColorRgba::new(0, 0, 0, 255));
                for (out, [r, g, b, a]) in image.pixels.iter_mut().zip(frame.to_rgba()) {
                    *out = ColorRgba::new(r, g, b, a);
                }
                if tx.send((index, image)).is_err() {
                    return;
                }
            }
            index += 1;
        }
    });
    rx
}

impl MpegPlayer {
    pub fn open(bytes: &[u8]) -> Option<Self> {
        let stream = Arc::new(ps::video_stream(bytes));
        let mut probe = Decoder::new(stream.to_vec());
        let (width, height) = probe.size()?;
        let fps = probe.frame_rate()?;
        drop(probe);
        let frames = spawn(stream.clone(), 0);
        // Show the first frame from the start, as the other backend does.
        let (shown, frame_buffer) = frames.recv().ok()?;
        Some(Self {
            stream,
            frames,
            fps,
            width,
            height,
            frame_buffer,
            shown,
            frame_serial: 1,
            ended: false,
        })
    }

    /// The frame due at `audio_time_seconds`.
    pub fn update(&mut self, audio_time_seconds: f64) -> Option<&ImageBuffer> {
        let due = (audio_time_seconds.max(0.0) * self.fps) as usize;
        // Back in time, or far ahead: restart the decoder where needed.
        let half_second = (self.fps / 2.0) as usize;
        let two_seconds = (self.fps * 2.0) as usize;
        let behind = self.shown != usize::MAX && due + half_second < self.shown;
        let far_ahead = self.shown != usize::MAX && due > self.shown + two_seconds;
        if behind || far_ahead {
            self.frames = spawn(self.stream.clone(), due);
            self.shown = usize::MAX;
            self.ended = false;
        }
        while !self.ended && (self.shown == usize::MAX || self.shown < due) {
            match self.frames.try_recv() {
                Ok((index, image)) => {
                    self.shown = index;
                    self.frame_buffer = image;
                    self.frame_serial += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => self.ended = true,
            }
        }
        Some(&self.frame_buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_containers_are_not_mpeg() {
        assert!(!is_mpeg(b"RIFF\0\0\0\0AVI "));
        assert!(is_mpeg(&[0, 0, 1, 0xba, 0x44]));
        assert!(is_mpeg(&[0, 0, 1, 0xb3, 0x10]));
    }
}
