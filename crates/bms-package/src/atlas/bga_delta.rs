use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

type FrameCandidate = (String, Vec<u8>, Option<String>, Option<u64>);

/// Type of frame in the BGA Delta bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BgaFrameType {
    /// Full, standalone raw image payload (Keyframe / I-frame).
    Keyframe,
    /// Byte-level XOR difference against a reference parent frame (Delta / P-frame).
    Delta,
}

/// Metadata for a single frame inside the BGA Delta bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BgaDeltaFrame {
    /// Frame compression type (Keyframe or Delta).
    pub frame_type: BgaFrameType,
    /// If this is a delta frame, the key of the reference parent frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Byte offset within the uncompressed bundle stream.
    pub offset: u64,
    /// Byte length within the bundle stream (raw bytes or XOR delta).
    pub length: u64,
    /// Original uncompressed file size in bytes.
    pub original_size: u64,
    /// Preserved original filename (e.g. "dream02.bmp"), for VFS and unpacking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,
}

/// Metadata describing the BGA Delta sequence bundle in a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BgaDeltaMeta {
    /// Relative path to the delta bundle binary within the package (e.g. "visual/bga_delta.bin").
    pub file: String,
    /// Total number of frames in the bundle.
    pub total_frames: u32,
    /// Total uncompressed byte size of all frames combined.
    pub total_raw_size: u64,
    /// Mapping from frame key or filename to its delta frame entry.
    pub frames: BTreeMap<String, BgaDeltaFrame>,
}

impl BgaDeltaMeta {
    pub fn new(
        file: impl Into<String>,
        total_raw_size: u64,
        frames: BTreeMap<String, BgaDeltaFrame>,
    ) -> Self {
        let total_frames = frames.len() as u32;
        Self {
            file: file.into(),
            total_frames,
            total_raw_size,
            frames,
        }
    }

    /// Validates internal consistency of the frame offsets and dependency graph.
    pub fn validate(&self) -> Result<(), String> {
        if self.file.is_empty() {
            return Err("BGA delta file path cannot be empty".to_string());
        }

        for (key, frame) in &self.frames {
            if let Some(ref parent_key) = frame.parent {
                if !self.frames.contains_key(parent_key) {
                    return Err(format!(
                        "Frame '{key}' references missing parent '{parent_key}'"
                    ));
                }
                if parent_key == key {
                    return Err(format!("Frame '{key}' cannot reference itself as parent"));
                }
            }
        }

        Ok(())
    }

    /// Unpacks all original frame byte vectors from the bundle stream in dependency order.
    pub fn unpack_all(&self, bundle_bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
        let mut reconstructed: BTreeMap<String, Vec<u8>> = BTreeMap::new();

        // Process keyframes first, then resolve deltas iteratively
        let mut remaining: Vec<(&String, &BgaDeltaFrame)> = self.frames.iter().collect();

        // Safety limit against circular dependencies
        let mut iterations = 0;
        let max_iterations = self.frames.len() + 1;

        while !remaining.is_empty() {
            iterations += 1;
            if iterations > max_iterations {
                return Err("Circular dependency detected in BGA delta frames".to_string());
            }

            let mut next_remaining = Vec::new();
            let mut progress = false;

            for (key, frame) in remaining {
                let start = frame.offset as usize;
                let end = start + frame.length as usize;
                if end > bundle_bytes.len() {
                    return Err(format!(
                        "Frame '{key}' slice [{start}..{end}] exceeds bundle size {}",
                        bundle_bytes.len()
                    ));
                }

                let chunk = &bundle_bytes[start..end];

                match frame.frame_type {
                    BgaFrameType::Keyframe => {
                        reconstructed.insert(key.clone(), chunk.to_vec());
                        progress = true;
                    }
                    BgaFrameType::Delta => {
                        let parent_key = frame
                            .parent
                            .as_ref()
                            .ok_or_else(|| format!("Delta frame '{key}' missing parent"))?;

                        if let Some(parent_bytes) = reconstructed.get(parent_key) {
                            if parent_bytes.len() != chunk.len() {
                                return Err(format!(
                                    "Delta frame '{key}' length {} does not match parent '{}' length {}",
                                    chunk.len(),
                                    parent_key,
                                    parent_bytes.len()
                                ));
                            }

                            // Reconstruct: curr = parent ^ xor_delta
                            let mut restored = Vec::with_capacity(chunk.len());
                            for i in 0..chunk.len() {
                                restored.push(parent_bytes[i] ^ chunk[i]);
                            }
                            reconstructed.insert(key.clone(), restored);
                            progress = true;
                        } else {
                            // Parent not yet resolved, keep for next iteration
                            next_remaining.push((key, frame));
                        }
                    }
                }
            }

            if !progress && !next_remaining.is_empty() {
                return Err("Failed to resolve parent dependencies for delta frames".to_string());
            }

            remaining = next_remaining;
        }

        Ok(reconstructed)
    }

    /// Unpacks a single frame by key or filename.
    /// If the frame is a delta, its ancestor chain is resolved and XOR applied.
    pub fn unpack_frame(
        &self,
        target_key: &str,
        bundle_bytes: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        let matched_key = if self.frames.contains_key(target_key) {
            Some(target_key)
        } else {
            self.frames
                .keys()
                .find(|k| {
                    k.eq_ignore_ascii_case(target_key)
                        || Path::new(k)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|fn_str| fn_str.eq_ignore_ascii_case(target_key))
                            .unwrap_or(false)
                })
                .map(|s| s.as_str())
        };

        let Some(key) = matched_key else {
            return Ok(None);
        };

        // Trace ancestor chain: [target, parent, grandparent, ..., root_keyframe]
        let mut chain = Vec::new();
        let mut curr_key = key;
        let mut visited = std::collections::HashSet::new();

        loop {
            if !visited.insert(curr_key) {
                return Err(format!("Circular reference detected in frame '{curr_key}'"));
            }
            let frame = self
                .frames
                .get(curr_key)
                .ok_or_else(|| format!("Missing frame '{curr_key}' in dependency chain"))?;
            chain.push((curr_key, frame));

            match frame.frame_type {
                BgaFrameType::Keyframe => break,
                BgaFrameType::Delta => {
                    let parent = frame
                        .parent
                        .as_deref()
                        .ok_or_else(|| format!("Delta frame '{curr_key}' missing parent"))?;
                    curr_key = parent;
                }
            }
        }

        // Chain is [target, ..., root]. Reverse to [root, ..., target]
        chain.reverse();

        // Start with root keyframe
        let (_, root_frame) = chain[0];
        let root_start = root_frame.offset as usize;
        let root_end = root_start + root_frame.length as usize;
        if root_end > bundle_bytes.len() {
            return Err(format!(
                "Root frame slice [{root_start}..{root_end}] exceeds bundle size {}",
                bundle_bytes.len()
            ));
        }
        let mut current_bytes = bundle_bytes[root_start..root_end].to_vec();

        // Apply deltas in order
        for &(delta_key, delta_frame) in &chain[1..] {
            let start = delta_frame.offset as usize;
            let end = start + delta_frame.length as usize;
            if end > bundle_bytes.len() {
                return Err(format!(
                    "Delta frame '{delta_key}' slice [{start}..{end}] exceeds bundle size {}",
                    bundle_bytes.len()
                ));
            }
            let delta_slice = &bundle_bytes[start..end];
            if delta_slice.len() != current_bytes.len() {
                return Err(format!(
                    "Delta frame '{delta_key}' length {} does not match parent length {}",
                    delta_slice.len(),
                    current_bytes.len()
                ));
            }
            for i in 0..current_bytes.len() {
                current_bytes[i] ^= delta_slice[i];
            }
        }

        Ok(Some(current_bytes))
    }
}

/// Splits a filename into base sequence prefix and numeric suffix for animation loop detection.
/// E.g. "dream02.bmp" -> ("dream", Some(2)), "mp_a_012.bmp" -> ("mp_a_", Some(12)).
pub fn split_sequence_prefix_and_num(name: &str) -> (String, Option<u64>) {
    let norm = name.replace('\\', "/");
    let filename = Path::new(&norm)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&norm);

    let stem = Path::new(filename)
        .file_stem()
        .and_then(|st| st.to_str())
        .unwrap_or(filename);

    let digits_count = stem
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .count();
    if digits_count > 0 && digits_count < stem.len() {
        let (pfx, digits) = stem.split_at(stem.len() - digits_count);
        if let Ok(num) = digits.parse::<u64>() {
            return (pfx.to_ascii_lowercase(), Some(num));
        }
    }

    (stem.to_ascii_lowercase(), None)
}

/// Default minimum byte identity percentage required to encode a frame as a Delta.
/// Empirically verified: Deflate (LZ77) on XOR deltas achieves positive compression
/// ratio only when identical bytes exceed 70%. Below 70%, XOR differences introduce
/// high-entropy pseudo-random noise that degrades 2D run-length coherence.
pub const DEFAULT_MIN_IDENTITY_PERCENT: usize = 70;

/// Builder that analyzes BGA image files, detects sequence loops, and produces
/// a compact Keyframe + XOR Delta bundle binary.
#[derive(Debug)]
pub struct BgaDeltaBuilder {
    frames: Vec<(String, Vec<u8>, Option<String>)>, // (key, raw_bytes, original_filename)
    min_identity_percent: usize,
}

impl Default for BgaDeltaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl BgaDeltaBuilder {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            min_identity_percent: DEFAULT_MIN_IDENTITY_PERCENT,
        }
    }

    /// Customizes the minimum byte identity percentage for delta encoding (default: 70).
    pub fn with_min_identity_percent(mut self, percent: usize) -> Self {
        self.min_identity_percent = percent.min(100);
        self
    }

    /// Adds a raw image file to be packed into the delta bundle.
    pub fn add_frame(
        &mut self,
        key: impl Into<String>,
        raw_bytes: Vec<u8>,
        original_filename: Option<String>,
    ) {
        self.frames.push((key.into(), raw_bytes, original_filename));
    }

    /// Whether any frames have been added.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Number of frames added.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Builds the BGA Delta bundle binary and metadata.
    ///
    /// Groups frames by sequence prefix and compares consecutive frames. If XOR delta
    /// yields significant zero runs (>= 70% identical bytes by default), encodes as Delta frame;
    /// otherwise encodes as standalone Keyframe to prevent entropy inflation.
    pub fn build(mut self, file_path: impl Into<String>) -> (BgaDeltaMeta, Vec<u8>) {
        let file = file_path.into();
        if self.frames.is_empty() {
            return (BgaDeltaMeta::new(file, 0, BTreeMap::new()), Vec::new());
        }

        // 1. Sort deterministically by key for reproducible packaging (INV-6)
        self.frames.sort_by(|a, b| a.0.cmp(&b.0));

        // 2. Group frames by sequence prefix (e.g. "dream", "mp_a_", "00_")
        // Preserve sorting within groups by numeric suffix or full key
        let mut groups: BTreeMap<String, Vec<FrameCandidate>> = BTreeMap::new();

        for (key, data, orig) in self.frames {
            let ref_name = orig.as_deref().unwrap_or(&key);
            let (prefix, num_opt) = split_sequence_prefix_and_num(ref_name);
            groups
                .entry(prefix)
                .or_default()
                .push((key, data, orig, num_opt));
        }

        let mut bundle_bytes = Vec::new();
        let mut delta_frames = BTreeMap::new();
        let mut total_raw_size = 0u64;

        for (_prefix, mut list) in groups {
            // Sort by numeric sequence index within the group
            list.sort_by(|a, b| match (a.3, b.3) {
                (Some(n1), Some(n2)) => n1.cmp(&n2),
                _ => a.0.cmp(&b.0),
            });

            let mut prev_key: Option<String> = None;
            let mut prev_data: Option<Vec<u8>> = None;

            for (key, data, orig, _num) in list {
                let orig_len = data.len() as u64;
                total_raw_size += orig_len;

                let mut is_delta = false;
                let mut xor_payload = Vec::new();
                let mut parent_candidate = None;

                if let (Some(ref p_key), Some(ref p_data)) = (&prev_key, &prev_data) {
                    if p_data.len() == data.len() && !data.is_empty() {
                        let mut zero_count = 0usize;
                        let mut xor = Vec::with_capacity(data.len());
                        for i in 0..data.len() {
                            let diff = data[i] ^ p_data[i];
                            if diff == 0 {
                                zero_count += 1;
                            }
                            xor.push(diff);
                        }

                        // Only encode as Delta if identical byte percentage meets the threshold (>= 70%)
                        if (zero_count * 100) / data.len() >= self.min_identity_percent {
                            is_delta = true;
                            xor_payload = xor;
                            parent_candidate = Some(p_key.clone());
                        }
                    }
                }

                let offset = bundle_bytes.len() as u64;
                let (frame_type, payload, parent) = if is_delta {
                    let p = parent_candidate;
                    (BgaFrameType::Delta, xor_payload, p)
                } else {
                    (BgaFrameType::Keyframe, data.clone(), None)
                };

                let length = payload.len() as u64;
                bundle_bytes.extend_from_slice(&payload);

                let meta_frame = BgaDeltaFrame {
                    frame_type,
                    parent,
                    offset,
                    length,
                    original_size: orig_len,
                    original_filename: orig,
                };

                // Remember this frame as previous for the next frame in the sequence
                prev_key = Some(key.clone());
                prev_data = Some(data);

                delta_frames.insert(key, meta_frame);
            }
        }

        let meta = BgaDeltaMeta::new(file, total_raw_size, delta_frames);
        (meta, bundle_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_sequence_prefix_and_num() {
        assert_eq!(
            split_sequence_prefix_and_num("dream01.bmp"),
            ("dream".to_string(), Some(1))
        );
        assert_eq!(
            split_sequence_prefix_and_num("dream12.bmp"),
            ("dream".to_string(), Some(12))
        );
        assert_eq!(
            split_sequence_prefix_and_num("mp_a_005.bmp"),
            ("mp_a_".to_string(), Some(5))
        );
        assert_eq!(
            split_sequence_prefix_and_num("00_31.bmp"),
            ("00_".to_string(), Some(31))
        );
        assert_eq!(
            split_sequence_prefix_and_num("stagefile.bmp"),
            ("stagefile".to_string(), None)
        );
        assert_eq!(
            split_sequence_prefix_and_num("banner.png"),
            ("banner".to_string(), None)
        );
    }

    #[test]
    fn test_bga_delta_builder_and_unpack_roundtrip() {
        let mut builder = BgaDeltaBuilder::new();

        // Create a simulated 3-frame sequence where only a small portion changes
        let frame1 = vec![10u8; 1000];
        let mut frame2 = frame1.clone();
        frame2[100..150].fill(99); // 50 bytes changed out of 1000 (95% identical)
        let mut frame3 = frame2.clone();
        frame3[500..550].fill(77); // 50 bytes changed out of 1000 (95% identical)

        // Standalone image
        let banner = vec![42u8; 300];

        builder.add_frame("seq01.bmp", frame1.clone(), Some("seq01.bmp".to_string()));
        builder.add_frame("seq02.bmp", frame2.clone(), Some("seq02.bmp".to_string()));
        builder.add_frame("seq03.bmp", frame3.clone(), Some("seq03.bmp".to_string()));
        builder.add_frame("banner.png", banner.clone(), Some("banner.png".to_string()));

        let (meta, bundle_bytes) = builder.build("visual/bga_delta.bin");

        assert!(meta.validate().is_ok());
        assert_eq!(meta.total_frames, 4);
        assert_eq!(meta.total_raw_size, 3300);

        // Verify frame types
        let f1 = meta.frames.get("seq01.bmp").unwrap();
        assert_eq!(f1.frame_type, BgaFrameType::Keyframe);
        assert!(f1.parent.is_none());

        let f2 = meta.frames.get("seq02.bmp").unwrap();
        assert_eq!(f2.frame_type, BgaFrameType::Delta);
        assert_eq!(f2.parent.as_deref(), Some("seq01.bmp"));

        let f3 = meta.frames.get("seq03.bmp").unwrap();
        assert_eq!(f3.frame_type, BgaFrameType::Delta);
        assert_eq!(f3.parent.as_deref(), Some("seq02.bmp"));

        let f_banner = meta.frames.get("banner.png").unwrap();
        assert_eq!(f_banner.frame_type, BgaFrameType::Keyframe);
        assert!(f_banner.parent.is_none());

        // Test roundtrip unpacking
        let unpacked = meta
            .unpack_all(&bundle_bytes)
            .expect("unpack_all should succeed");
        assert_eq!(unpacked.len(), 4);
        assert_eq!(unpacked.get("seq01.bmp").unwrap(), &frame1);
        assert_eq!(unpacked.get("seq02.bmp").unwrap(), &frame2);
        assert_eq!(unpacked.get("seq03.bmp").unwrap(), &frame3);
        assert_eq!(unpacked.get("banner.png").unwrap(), &banner);

        // Test single frame unpacking
        assert_eq!(
            meta.unpack_frame("seq01.bmp", &bundle_bytes)
                .unwrap()
                .unwrap(),
            frame1
        );
        assert_eq!(
            meta.unpack_frame("seq02.bmp", &bundle_bytes)
                .unwrap()
                .unwrap(),
            frame2
        );
        assert_eq!(
            meta.unpack_frame("seq03.bmp", &bundle_bytes)
                .unwrap()
                .unwrap(),
            frame3
        );
        assert_eq!(
            meta.unpack_frame("banner.png", &bundle_bytes)
                .unwrap()
                .unwrap(),
            banner
        );
        assert!(meta
            .unpack_frame("nonexistent.bmp", &bundle_bytes)
            .unwrap()
            .is_none());

        // Serialization roundtrip
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: BgaDeltaMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, deserialized);
    }
}
