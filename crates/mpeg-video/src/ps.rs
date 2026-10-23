//! MPEG program streams (`.mpg`, ISO/IEC 11172-1 and 13818-1): the video
//! elementary stream out of the packs and PES packets.

const PACK: u8 = 0xba;
const SYSTEM_HEADER: u8 = 0xbb;
const PROGRAM_END: u8 = 0xb9;
const SEQUENCE_HEADER: u8 = 0xb3;

fn start_code_at(data: &[u8], at: usize) -> Option<u8> {
    (data.get(at..at + 3)? == [0, 0, 1]).then(|| data.get(at + 3).copied())?
}

/// Whether `data` looks like MPEG video: a program stream pack or a video
/// sequence header first.
pub fn is_mpeg(data: &[u8]) -> bool {
    matches!(start_code_at(data, 0), Some(PACK | SEQUENCE_HEADER))
}

/// The first video stream's elementary stream. A stream that starts with a
/// sequence header is returned as it is.
pub fn video_stream(data: &[u8]) -> Vec<u8> {
    if start_code_at(data, 0) == Some(SEQUENCE_HEADER) {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len());
    let mut stream: Option<u8> = None;
    let mut at = 0;
    while at + 4 <= data.len() {
        let Some(code) = start_code_at(data, at) else {
            // Lost: find the next start code.
            at += 1;
            continue;
        };
        match code {
            PACK => {
                let Some(&b) = data.get(at + 4) else { break };
                at += if b >> 6 == 1 {
                    // MPEG-2: 14 bytes and stuffing.
                    14 + (data.get(at + 13).copied().unwrap_or(0) & 7) as usize
                } else {
                    12
                };
            }
            PROGRAM_END => at += 4,
            SYSTEM_HEADER | 0xbc..=0xff => {
                let Some(len) = data.get(at + 4..at + 6) else {
                    break;
                };
                let len = u16::from_be_bytes([len[0], len[1]]) as usize;
                let end = (at + 6 + len).min(data.len());
                if (0xe0..=0xef).contains(&code) && *stream.get_or_insert(code) == code {
                    if let Some(payload) = pes_payload(&data[at + 6..end]) {
                        out.extend_from_slice(payload);
                    }
                }
                at = end;
            }
            _ => at += 1,
        }
    }
    out
}

/// The payload of a PES packet body (after its length field): MPEG-2 headers
/// say their own length; MPEG-1 ones are stuffing, an optional buffer size
/// and the time stamps.
fn pes_payload(body: &[u8]) -> Option<&[u8]> {
    if body.first()? >> 6 == 0b10 {
        let header = *body.get(2)? as usize;
        return body.get(3 + header..);
    }
    let mut i = 0;
    while *body.get(i)? == 0xff {
        i += 1;
    }
    if body[i] >> 6 == 0b01 {
        i += 2;
    }
    let flags = *body.get(i)? >> 4;
    i += match flags {
        0b0010 => 5,
        0b0011 => 10,
        _ => 1,
    };
    body.get(i..)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pes(stream: u8, header: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut p = vec![0, 0, 1, stream];
        let len = (header.len() + payload.len()) as u16;
        p.extend(len.to_be_bytes());
        p.extend(header);
        p.extend(payload);
        p
    }

    #[test]
    fn video_payloads_are_joined_across_packs() {
        let mut data = vec![0, 0, 1, 0xba, 0x44, 0, 0, 0, 0, 0, 0, 0, 0, 0xf8]; // MPEG-2 pack
        data.extend(pes(0xe0, &[0x80, 0x00, 0x00], &[0, 0, 1, 0xb3, 1]));
        data.extend(pes(0xc0, &[0x80, 0x00, 0x00], &[9, 9])); // audio
        data.extend(pes(0xe0, &[0x80, 0x80, 0x05, 0x21, 0, 1, 0, 1], &[2, 3]));
        data.extend([0, 0, 1, 0xb9]);
        assert!(is_mpeg(&data));
        assert_eq!(video_stream(&data), [0, 0, 1, 0xb3, 1, 2, 3]);
    }

    #[test]
    fn mpeg1_packets_skip_stuffing_and_time_stamps() {
        let mut data = vec![0, 0, 1, 0xba, 0x21, 0, 1, 0, 1, 0x80, 0, 1]; // MPEG-1 pack
        data.extend(pes(0xe0, &[0xff, 0x40, 0x10, 0x21, 0, 1, 0, 1], &[7, 8]));
        data.extend(pes(0xe0, &[0x0f], &[9]));
        assert_eq!(video_stream(&data), [7, 8, 9]);
    }
}
