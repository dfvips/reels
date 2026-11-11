//! SM3（GB/T 32905-2016）实现。

const IV: [u32; 8] = [
    0x7380_166F,
    0x4914_B2B9,
    0x1724_42D7,
    0xDA8A_0600,
    0xA96F_30BC,
    0x1631_38AA,
    0xE38D_EE4D,
    0xB0FB_0E4E,
];

const T0: u32 = 0x79CC_4519;
const T1: u32 = 0x7A87_9D8A;

#[inline]
fn rotl(value: u32, bits: u32) -> u32 {
    let bits = bits % 32;
    value.rotate_left(bits)
}

#[inline]
fn ff(index: usize, x: u32, y: u32, z: u32) -> u32 {
    if index < 16 {
        x ^ y ^ z
    } else {
        (x & y) | (x & z) | (y & z)
    }
}

#[inline]
fn gg(index: usize, x: u32, y: u32, z: u32) -> u32 {
    if index < 16 {
        x ^ y ^ z
    } else {
        (x & y) | ((!x) & z)
    }
}

#[inline]
fn p1(x: u32) -> u32 {
    x ^ rotl(x, 15) ^ rotl(x, 23)
}

fn expand(block: &[u8]) -> ([u32; 68], [u32; 64]) {
    let mut w = [0u32; 68];
    for (j, slot) in w.iter_mut().take(16).enumerate() {
        let i = j * 4;
        *slot = u32::from_be_bytes([block[i], block[i + 1], block[i + 2], block[i + 3]]);
    }
    for j in 16..68 {
        let x = w[j - 16] ^ w[j - 9] ^ rotl(w[j - 3], 15);
        w[j] = p1(x) ^ rotl(w[j - 13], 7) ^ w[j - 6];
    }
    let mut w1 = [0u32; 64];
    for j in 0..64 {
        w1[j] = w[j] ^ w[j + 4];
    }
    (w, w1)
}

fn compress(state: &mut [u32; 8], block: &[u8]) {
    let (w, w1) = expand(block);
    let (mut a, mut b, mut c, mut d) = (state[0], state[1], state[2], state[3]);
    let (mut e, mut f, mut g, mut h) = (state[4], state[5], state[6], state[7]);
    for j in 0..64 {
        let t = if j < 16 { T0 } else { T1 };
        let ss1 = rotl(
            rotl(a, 12).wrapping_add(e).wrapping_add(rotl(t, j as u32)),
            7,
        );
        let ss2 = ss1 ^ rotl(a, 12);
        let tt1 = ff(j, a, b, c)
            .wrapping_add(d)
            .wrapping_add(ss2)
            .wrapping_add(w1[j]);
        let tt2 = gg(j, e, f, g)
            .wrapping_add(h)
            .wrapping_add(ss1)
            .wrapping_add(w[j]);
        d = c;
        c = rotl(b, 9);
        b = a;
        a = tt1;
        h = g;
        g = rotl(f, 19);
        f = e;
        e = tt2 ^ rotl(tt2, 9) ^ rotl(tt2, 17);
    }
    state[0] ^= a;
    state[1] ^= b;
    state[2] ^= c;
    state[3] ^= d;
    state[4] ^= e;
    state[5] ^= f;
    state[6] ^= g;
    state[7] ^= h;
}

/// 计算 SM3 摘要（32 字节）。
pub fn sm3_hash(message: &[u8]) -> [u8; 32] {
    let bit_length = (message.len() as u64) * 8;
    let mut padded = Vec::with_capacity(message.len() + 72);
    padded.extend_from_slice(message);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_length.to_be_bytes());

    let mut state = IV;
    for block in padded.chunks(64) {
        compress(&mut state, block);
    }

    let mut out = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// 字符串按 UTF-8 编码后求摘要。
pub fn sm3_hash_str(text: &str) -> [u8; 32] {
    sm3_hash(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn sm3_reference_vectors() {
        assert_eq!(
            hex(&sm3_hash(b"abc")),
            "66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0"
        );
        assert_eq!(
            hex(&sm3_hash("abcd".repeat(16).as_bytes())),
            "debe9ff92275b8a138604889c18e5a4d6fdb70e5387e5765293dcba39c0c5732"
        );
    }
}
