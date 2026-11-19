//! 음성 청크의 base64 디코딩, TTS 오디오의 base64 인코딩, PCM16을 WAV로 감싸기.
//!
//! base64는 Java `Base64.getDecoder()`/`getEncoder()`(RFC 4648 기본 알파벳)와 같게 동작한다.
//! 디코딩: 끝의 `=`는 없어도 되지만 있으면 올바른 자리여야 하고, 그 뒤에 글자가 오면 안 된다.
//! 알파벳 밖의 글자(공백 포함)는 거절한다. 남는 비트는 검사하지 않는다.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(super) const DEFAULT_SAMPLE_RATE: i32 = 16000;

/// base64 문자열을 디코딩한다. 올바르지 않으면 `None`.
pub(super) fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let src = text.as_bytes();
    let mut out = Vec::with_capacity(src.len() / 4 * 3);
    let mut bits: u32 = 0;
    let mut count = 0;
    let mut index = 0;
    while index < src.len() {
        let byte = src[index];
        index += 1;
        if byte == b'=' {
            // 한 단위의 3번째 자리 뒤의 `=`는 하나 더 있어야 한다. 단위 첫 자리의 `=`는 틀렸다.
            if count == 0 || count == 1 {
                return None;
            }
            if count == 2 {
                if index >= src.len() || src[index] != b'=' {
                    return None;
                }
                index += 1;
            }
            if index < src.len() {
                return None;
            }
            break;
        }
        let value = sextet(byte)?;
        bits = (bits << 6) | u32::from(value);
        count += 1;
        if count == 4 {
            out.push((bits >> 16) as u8);
            out.push((bits >> 8) as u8);
            out.push(bits as u8);
            bits = 0;
            count = 0;
        }
    }
    match count {
        0 => {}
        2 => out.push((bits >> 4) as u8),
        3 => {
            out.push((bits >> 10) as u8);
            out.push((bits >> 2) as u8);
        }
        _ => return None,
    }
    Some(out)
}

fn sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// `=`로 채우는 표준 base64.
pub(super) fn encode_base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let bits = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(bits >> 18) as usize & 63] as char);
        out.push(ALPHABET[(bits >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(bits >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[bits as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// 모노 16비트 PCM 앞에 44바이트 WAV 헤더를 붙인다. 샘플레이트가 0 이하면 16000이다.
pub(super) fn to_wav(pcm16: &[u8], sample_rate: i32) -> Vec<u8> {
    let rate = if sample_rate > 0 {
        sample_rate
    } else {
        DEFAULT_SAMPLE_RATE
    };
    let data_size = pcm16.len() as i32;
    let mut wav = Vec::with_capacity(44 + pcm16.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36i32.wrapping_add(data_size)).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16i32.to_le_bytes());
    wav.extend_from_slice(&1i16.to_le_bytes());
    wav.extend_from_slice(&1i16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&rate.wrapping_mul(2).to_le_bytes());
    wav.extend_from_slice(&2i16.to_le_bytes());
    wav.extend_from_slice(&16i16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm16);
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_accepts_padded_and_unpadded_input() {
        assert_eq!(Some(vec![0, 1, 2, 3]), decode_base64("AAECAw=="));
        assert_eq!(Some(vec![0, 1, 2, 3]), decode_base64("AAECAw"));
        assert_eq!(Some(vec![0, 1, 2]), decode_base64("AAEC"));
        assert_eq!(Some(vec![0, 1, 2, 3, 4]), decode_base64("AAECAwQ="));
        assert_eq!(Some(vec![]), decode_base64(""));
    }

    #[test]
    fn decode_rejects_what_java_rejects() {
        assert_eq!(None, decode_base64("AAECAw="));
        assert_eq!(None, decode_base64("AAECAw==A"));
        assert_eq!(None, decode_base64("AAEC A=="));
        assert_eq!(None, decode_base64("A"));
        assert_eq!(None, decode_base64("=AAA"));
        assert_eq!(None, decode_base64("AAECA==="));
        assert_eq!(None, decode_base64("not base64!"));
    }

    #[test]
    fn encode_matches_java_encoder() {
        assert_eq!("AQIDBA==", encode_base64(&[1, 2, 3, 4]));
        assert_eq!("AQID", encode_base64(&[1, 2, 3]));
        assert_eq!("AQI=", encode_base64(&[1, 2]));
        assert_eq!("", encode_base64(&[]));
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(Some(bytes.clone()), decode_base64(&encode_base64(&bytes)));
    }

    #[test]
    fn to_wav_writes_little_endian_pcm_header() {
        let wav = to_wav(&[9, 8, 7, 6], 0);

        assert_eq!(48, wav.len());
        assert_eq!(b"RIFF", &wav[0..4]);
        assert_eq!(40, i32::from_le_bytes(wav[4..8].try_into().unwrap()));
        assert_eq!(b"WAVEfmt ", &wav[8..16]);
        assert_eq!(16000, i32::from_le_bytes(wav[24..28].try_into().unwrap()));
        assert_eq!(32000, i32::from_le_bytes(wav[28..32].try_into().unwrap()));
        assert_eq!(b"data", &wav[36..40]);
        assert_eq!(4, i32::from_le_bytes(wav[40..44].try_into().unwrap()));
        assert_eq!(&[9, 8, 7, 6], &wav[44..]);
    }
}
