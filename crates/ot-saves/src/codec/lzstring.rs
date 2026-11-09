//! LZ-String's `compressToBase64` / `decompressFromBase64`, as used by RPG
//! Maker MV for its `.rpgsave` files (JSON compressed with lz-string, then
//! written as Base64 text). The output matches the JavaScript library bit for
//! bit, so a save written back is what the game itself would write.
//!
//! The library works on UTF-16 code units (JavaScript strings); so does this.

use std::collections::HashMap;

const ALPHABET: &[u8; 65] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";

/// Decompressed text longer than this (in UTF-16 units) is refused.
const MAX_UNITS: usize = (super::layers::MAX_INFLATED / 2) as usize;

struct Bits {
    out: Vec<u8>,
    val: u32,
    pos: u32,
}

impl Bits {
    /// Writes `n` bits of `value`, lowest bit first.
    fn put(&mut self, n: u32, mut value: u32) {
        for _ in 0..n {
            self.val = (self.val << 1) | (value & 1);
            if self.pos == 5 {
                self.pos = 0;
                self.out.push(ALPHABET[self.val as usize]);
                self.val = 0;
            } else {
                self.pos += 1;
            }
            value >>= 1;
        }
    }
}

/// `LZString.compressToBase64(text)`.
pub fn compress_to_base64(text: &str) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut dict: HashMap<Vec<u16>, u32> = HashMap::new();
    let mut to_create: std::collections::HashSet<Vec<u16>> = std::collections::HashSet::new();
    let mut w: Vec<u16> = Vec::new();
    let mut enlarge_in: u32 = 2;
    let mut dict_size: u32 = 3;
    let mut num_bits: u32 = 2;
    let mut bits = Bits {
        out: Vec::new(),
        val: 0,
        pos: 0,
    };

    let emit_w = |w: &Vec<u16>,
                  bits: &mut Bits,
                  dict: &HashMap<Vec<u16>, u32>,
                  to_create: &mut std::collections::HashSet<Vec<u16>>,
                  enlarge_in: &mut u32,
                  num_bits: &mut u32| {
        if to_create.contains(w) {
            let c = u32::from(w[0]);
            if c < 256 {
                bits.put(*num_bits, 0);
                bits.put(8, c);
            } else {
                bits.put(*num_bits, 1);
                bits.put(16, c);
            }
            *enlarge_in -= 1;
            if *enlarge_in == 0 {
                *enlarge_in = 1 << *num_bits;
                *num_bits += 1;
            }
            to_create.remove(w);
        } else {
            bits.put(*num_bits, dict[w]);
        }
        *enlarge_in -= 1;
        if *enlarge_in == 0 {
            *enlarge_in = 1 << *num_bits;
            *num_bits += 1;
        }
    };

    for &c in &units {
        let key = vec![c];
        if !dict.contains_key(&key) {
            dict.insert(key.clone(), dict_size);
            dict_size += 1;
            to_create.insert(key);
        }
        let mut wc = w.clone();
        wc.push(c);
        if dict.contains_key(&wc) {
            w = wc;
        } else {
            emit_w(
                &w,
                &mut bits,
                &dict,
                &mut to_create,
                &mut enlarge_in,
                &mut num_bits,
            );
            dict.insert(wc, dict_size);
            dict_size += 1;
            w = vec![c];
        }
    }
    if !w.is_empty() {
        emit_w(
            &w,
            &mut bits,
            &dict,
            &mut to_create,
            &mut enlarge_in,
            &mut num_bits,
        );
    }
    // End of stream.
    bits.put(num_bits, 2);
    // Flush the last character.
    loop {
        bits.val <<= 1;
        if bits.pos == 5 {
            bits.out.push(ALPHABET[bits.val as usize]);
            break;
        }
        bits.pos += 1;
    }
    let mut s = String::from_utf8(bits.out).unwrap_or_default();
    match s.len() % 4 {
        1 => s.push_str("==="),
        2 => s.push_str("=="),
        3 => s.push('='),
        _ => {}
    }
    s
}

struct Reader<'a> {
    input: &'a [u8],
    val: u32,
    position: u32,
    index: usize,
}

impl Reader<'_> {
    fn value(&self, i: usize) -> u32 {
        self.input
            .get(i)
            .and_then(|c| ALPHABET.iter().position(|a| a == c))
            .map_or(0, |p| p as u32)
    }

    fn read(&mut self, n: u32) -> u32 {
        let mut bits = 0;
        let mut power = 1u32;
        for _ in 0..n {
            let resb = self.val & self.position;
            self.position >>= 1;
            if self.position == 0 {
                self.position = 32;
                self.val = self.value(self.index);
                self.index += 1;
            }
            if resb > 0 {
                bits |= power;
            }
            power = power.wrapping_shl(1);
        }
        bits
    }
}

/// `LZString.decompressFromBase64(text)`; `None` where the library gives
/// `null` or `""` (not lz-string data), or the text isn't valid UTF-16.
pub fn decompress_from_base64(text: &str) -> Option<String> {
    let input = text.as_bytes();
    if input.is_empty() || !input.iter().all(|c| ALPHABET.contains(c)) {
        return None;
    }
    let mut r = Reader {
        input,
        val: 0,
        position: 32,
        index: 1,
    };
    r.val = r.value(0);
    let mut dict: Vec<Vec<u16>> = vec![Vec::new(), Vec::new(), Vec::new()];
    let mut enlarge_in: u32 = 4;
    let mut num_bits: u32 = 3;
    let first = match r.read(2) {
        0 => r.read(8),
        1 => r.read(16),
        _ => return None,
    };
    let c = vec![first as u16];
    dict.push(c.clone());
    let mut w = c.clone();
    let mut result: Vec<u16> = c;
    loop {
        if r.index > input.len() {
            return None;
        }
        let mut code = r.read(num_bits) as usize;
        match code {
            0 | 1 => {
                let ch = r.read(if code == 0 { 8 } else { 16 }) as u16;
                dict.push(vec![ch]);
                code = dict.len() - 1;
                enlarge_in -= 1;
            }
            2 => return String::from_utf16(&result).ok(),
            _ => {}
        }
        if enlarge_in == 0 {
            enlarge_in = 1u32.checked_shl(num_bits)?;
            num_bits += 1;
        }
        let entry = if code < dict.len() && code > 2 {
            dict[code].clone()
        } else if code == dict.len() {
            let mut e = w.clone();
            e.push(*w.first()?);
            e
        } else {
            return None;
        };
        result.extend_from_slice(&entry);
        if result.len() > MAX_UNITS {
            return None;
        }
        let mut next = w;
        next.push(entry[0]);
        dict.push(next);
        enlarge_in -= 1;
        if enlarge_in == 0 {
            enlarge_in = 1u32.checked_shl(num_bits)?;
            num_bits += 1;
        }
        w = entry;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Made with the JavaScript lz-string 1.5 library (node_modules/lz-string).
    const VECTORS: &[(&str, &str)] = &[
        (
            r#"{"system":{"gold":250},"party":{"_actors":[1,2]}}"#,
            "N4IgzgnmAuCmC2IBcoDmB7ANgE2QJgFYAGAXwBoQAHAQwCdoJlQB9agY2nVrGQG0BGMngC6JEkA=",
        ),
        ("Hello, world", "BIUwNmD2A0AEDukBOYAmQ==="),
        (
            "日本語のセーブ {\"a\":1}",
            "qemhpzR5UYdgyN0Mg/DIawwAIDeAiAhigXARgL5A",
        ),
        ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab", "IY18ZkEZA==="),
    ];

    #[test]
    fn matches_the_javascript_library() {
        for (plain, packed) in VECTORS {
            assert_eq!(compress_to_base64(plain), *packed, "{plain}");
            assert_eq!(decompress_from_base64(packed).as_deref(), Some(*plain));
        }
    }

    #[test]
    fn round_trips_bigger_text() {
        let mut s = String::from("{\"actors\":[");
        for i in 0..2000 {
            s.push_str(&format!(
                "{{\"id\":{i},\"name\":\"Hero {}\",\"hp\":{}}},",
                i % 7,
                i * 3
            ));
        }
        s.push_str("null]}");
        assert_eq!(
            decompress_from_base64(&compress_to_base64(&s)).as_deref(),
            Some(s.as_str())
        );
    }

    #[test]
    fn other_text_is_not_lzstring() {
        assert_eq!(decompress_from_base64(""), None);
        assert_eq!(decompress_from_base64("not lzstring at all"), None);
        assert_eq!(decompress_from_base64("AAAA"), None);
    }
}
