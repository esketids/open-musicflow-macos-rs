//! Cleanroom FIPS-197 AES-256-CBC, LG TCP 9741 Wire Framing (0x10 / 0x00),
//! and RFC 6455 WebSocket SHA-1 + Base64 Handshake in pure Rust (zero external crates).

pub const LG_SJ8_V1_KEY: &[u8; 32] = b"4efgvbn m546Uy7kolKrftgbn =-0u&~";
pub const LG_SJ8_V1_IV: &[u8; 16] = b"54eRty@hkL,;/y9U";

pub const LG_TEMESCAL_V2_KEY: &[u8; 32] = b"T^&*J%^7tr~4^%^&I(o%^!jIJ__+a0 k";
pub const LG_TEMESCAL_V2_IV: &[u8; 16] = b"'%^Ur7gy$~t+f)%@";

pub const LG_MAGIC_ENCRYPTED: u8 = 0x10;
pub const LG_MAGIC_PLAIN: u8 = 0x00;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionMode {
    Sj8V1,
    TemescalV2,
    Plain,
}

impl EncryptionMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "temescal_v2" => EncryptionMode::TemescalV2,
            "plain" => EncryptionMode::Plain,
            _ => EncryptionMode::Sj8V1,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            EncryptionMode::Sj8V1 => "sj8_v1",
            EncryptionMode::TemescalV2 => "temescal_v2",
            EncryptionMode::Plain => "plain",
        }
    }
}

// FIPS-197 AES S-Box and Inverse S-Box
const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

const INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

const RCON: [u8; 15] = [
    0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36, 0x6c, 0xd8, 0xab, 0x4d,
];

#[inline]
fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    for _ in 0..8 {
        if (b & 1) != 0 {
            p ^= a;
        }
        let hi_bit = a & 0x80;
        a <<= 1;
        if hi_bit != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    p
}

fn expand_key_256(key: &[u8; 32]) -> [[u8; 16]; 15] {
    let mut w = [[0u8; 4]; 60];
    for i in 0..8 {
        w[i] = [key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]];
    }
    for i in 8..60 {
        let mut temp = w[i - 1];
        if i % 8 == 0 {
            temp = [
                SBOX[temp[1] as usize] ^ RCON[i / 8],
                SBOX[temp[2] as usize],
                SBOX[temp[3] as usize],
                SBOX[temp[0] as usize],
            ];
        } else if i % 8 == 4 {
            temp = [
                SBOX[temp[0] as usize],
                SBOX[temp[1] as usize],
                SBOX[temp[2] as usize],
                SBOX[temp[3] as usize],
            ];
        }
        for j in 0..4 {
            w[i][j] = w[i - 8][j] ^ temp[j];
        }
    }
    let mut round_keys = [[0u8; 16]; 15];
    for r in 0..15 {
        for c in 0..4 {
            for b in 0..4 {
                round_keys[r][4 * c + b] = w[4 * r + c][b];
            }
        }
    }
    round_keys
}

fn aes256_encrypt_block(block: &[u8; 16], rkeys: &[[u8; 16]; 15]) -> [u8; 16] {
    let mut s = *block;
    for i in 0..16 {
        s[i] ^= rkeys[0][i];
    }
    for rkey in rkeys.iter().take(14).skip(1) {
        for byte in &mut s {
            *byte = SBOX[*byte as usize];
        }
        // ShiftRows
        s = [
            s[0], s[5], s[10], s[15],
            s[4], s[9], s[14], s[3],
            s[8], s[13], s[2], s[7],
            s[12], s[1], s[6], s[11],
        ];
        // MixColumns
        for c in 0..4 {
            let idx = 4 * c;
            let (s0, s1, s2, s3) = (s[idx], s[idx + 1], s[idx + 2], s[idx + 3]);
            s[idx] = gmul(2, s0) ^ gmul(3, s1) ^ s2 ^ s3;
            s[idx + 1] = s0 ^ gmul(2, s1) ^ gmul(3, s2) ^ s3;
            s[idx + 2] = s0 ^ s1 ^ gmul(2, s2) ^ gmul(3, s3);
            s[idx + 3] = gmul(3, s0) ^ s1 ^ s2 ^ gmul(2, s3);
        }
        for i in 0..16 {
            s[i] ^= rkey[i];
        }
    }
    // Final round (14)
    for byte in &mut s {
        *byte = SBOX[*byte as usize];
    }
    s = [
        s[0], s[5], s[10], s[15],
        s[4], s[9], s[14], s[3],
        s[8], s[13], s[2], s[7],
        s[12], s[1], s[6], s[11],
    ];
    for i in 0..16 {
        s[i] ^= rkeys[14][i];
    }
    s
}

fn aes256_decrypt_block(block: &[u8; 16], rkeys: &[[u8; 16]; 15]) -> [u8; 16] {
    let mut s = *block;
    for i in 0..16 {
        s[i] ^= rkeys[14][i];
    }
    for round in (1..14).rev() {
        // InvShiftRows
        s = [
            s[0], s[13], s[10], s[7],
            s[4], s[1], s[14], s[11],
            s[8], s[5], s[2], s[15],
            s[12], s[9], s[6], s[3],
        ];
        for byte in &mut s {
            *byte = INV_SBOX[*byte as usize];
        }
        for i in 0..16 {
            s[i] ^= rkeys[round][i];
        }
        // InvMixColumns
        for c in 0..4 {
            let idx = 4 * c;
            let (s0, s1, s2, s3) = (s[idx], s[idx + 1], s[idx + 2], s[idx + 3]);
            s[idx] = gmul(0x0e, s0) ^ gmul(0x0b, s1) ^ gmul(0x0d, s2) ^ gmul(0x09, s3);
            s[idx + 1] = gmul(0x09, s0) ^ gmul(0x0e, s1) ^ gmul(0x0b, s2) ^ gmul(0x0d, s3);
            s[idx + 2] = gmul(0x0d, s0) ^ gmul(0x09, s1) ^ gmul(0x0e, s2) ^ gmul(0x0b, s3);
            s[idx + 3] = gmul(0x0b, s0) ^ gmul(0x0d, s1) ^ gmul(0x09, s2) ^ gmul(0x0e, s3);
        }
    }
    s = [
        s[0], s[13], s[10], s[7],
        s[4], s[1], s[14], s[11],
        s[8], s[5], s[2], s[15],
        s[12], s[9], s[6], s[3],
    ];
    for byte in &mut s {
        *byte = INV_SBOX[*byte as usize];
    }
    for i in 0..16 {
        s[i] ^= rkeys[0][i];
    }
    s
}

pub fn encrypt_aes256_cbc(plaintext: &[u8], key: &[u8; 32], iv: &[u8; 16]) -> Vec<u8> {
    let rkeys = expand_key_256(key);
    let pad_len = 16 - (plaintext.len() % 16);
    let mut padded = Vec::with_capacity(plaintext.len() + pad_len);
    padded.extend_from_slice(plaintext);
    padded.extend(std::iter::repeat_n(pad_len as u8, pad_len));

    let mut out = Vec::with_capacity(padded.len());
    let mut prev = *iv;
    for chunk in padded.as_chunks::<16>().0 {
        let mut block = [0u8; 16];
        for i in 0..16 {
            block[i] = chunk[i] ^ prev[i];
        }
        let enc = aes256_encrypt_block(&block, &rkeys);
        out.extend_from_slice(&enc);
        prev = enc;
    }
    out
}

pub fn decrypt_aes256_cbc(ciphertext: &[u8], key: &[u8; 32], iv: &[u8; 16]) -> Option<Vec<u8>> {
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        return None;
    }
    let rkeys = expand_key_256(key);
    let mut out = Vec::with_capacity(ciphertext.len());
    let mut prev = *iv;
    for chunk in ciphertext.as_chunks::<16>().0 {
        let block = *chunk;
        let mut dec = aes256_decrypt_block(&block, &rkeys);
        for i in 0..16 {
            dec[i] ^= prev[i];
        }
        out.extend_from_slice(&dec);
        prev = block;
    }
    let pad_len = *out.last()? as usize;
    if pad_len == 0 || pad_len > 16 || pad_len > out.len() {
        return None;
    }
    for &b in &out[out.len() - pad_len..] {
        if b as usize != pad_len {
            return None;
        }
    }
    out.truncate(out.len() - pad_len);
    Some(out)
}

pub fn encode_lg_packet(json_text: &str, mode: EncryptionMode) -> Vec<u8> {
    match mode {
        EncryptionMode::Plain => {
            let bytes = json_text.as_bytes();
            let mut out = Vec::with_capacity(5 + bytes.len());
            out.push(LG_MAGIC_PLAIN);
            out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            out.extend_from_slice(bytes);
            out
        }
        EncryptionMode::Sj8V1 => {
            // MusicFlow_Windows appends \r on V1 encrypted frames
            let mut body = String::with_capacity(json_text.len() + 1);
            body.push_str(json_text);
            if !body.ends_with('\r') {
                body.push('\r');
            }
            let enc = encrypt_aes256_cbc(body.as_bytes(), LG_SJ8_V1_KEY, LG_SJ8_V1_IV);
            let mut out = Vec::with_capacity(5 + enc.len());
            out.push(LG_MAGIC_ENCRYPTED);
            out.extend_from_slice(&(enc.len() as u32).to_be_bytes());
            out.extend_from_slice(&enc);
            out
        }
        EncryptionMode::TemescalV2 => {
            let enc = encrypt_aes256_cbc(json_text.as_bytes(), LG_TEMESCAL_V2_KEY, LG_TEMESCAL_V2_IV);
            let mut out = Vec::with_capacity(5 + enc.len());
            out.push(LG_MAGIC_ENCRYPTED);
            out.extend_from_slice(&(enc.len() as u32).to_be_bytes());
            out.extend_from_slice(&enc);
            out
        }
    }
}

pub fn decrypt_lg_payload_auto(payload: &[u8]) -> Option<(String, EncryptionMode)> {
    for (mode, key, iv) in [
        (EncryptionMode::Sj8V1, LG_SJ8_V1_KEY, LG_SJ8_V1_IV),
        (EncryptionMode::TemescalV2, LG_TEMESCAL_V2_KEY, LG_TEMESCAL_V2_IV),
    ] {
        if let Some(bytes) = decrypt_aes256_cbc(payload, key, iv) {
            if let Ok(text) = String::from_utf8(bytes) {
                let trimmed = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
                if trimmed.starts_with('{') && trimmed.ends_with('}') {
                    return Some((trimmed.to_string(), mode));
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct DecodedLgFrame {
    pub encrypted: bool,
    pub cipher_mode: EncryptionMode,
    pub raw_byte_length: usize,
    pub json_text: String,
}

pub struct LgPacketFramer {
    buf: Vec<u8>,
}

impl LgPacketFramer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Vec<DecodedLgFrame> {
        self.buf.extend_from_slice(chunk);
        let mut frames = Vec::new();

        while !self.buf.is_empty() {
            let first = self.buf[0];
            if first == LG_MAGIC_ENCRYPTED || first == LG_MAGIC_PLAIN || first == 0x11 || first == 0x01 {
                if self.buf.len() < 5 {
                    break;
                }
                let len = u32::from_be_bytes([self.buf[1], self.buf[2], self.buf[3], self.buf[4]]) as usize;
                if len > 512 * 1024 {
                    self.buf.remove(0);
                    continue;
                }
                if self.buf.len() < 5 + len {
                    break;
                }
                let payload = self.buf[5..5 + len].to_vec();
                self.buf.drain(0..5 + len);

                if (first & 0x0F) == 0x01 || len == 0 {
                    continue;
                }

                if first == LG_MAGIC_ENCRYPTED {
                    if let Some((json_text, mode)) = decrypt_lg_payload_auto(&payload) {
                        frames.push(DecodedLgFrame {
                            encrypted: true,
                            cipher_mode: mode,
                            raw_byte_length: 5 + len,
                            json_text,
                        });
                    }
                } else if let Ok(text) = String::from_utf8(payload) {
                    let trimmed = text.trim().to_string();
                    frames.push(DecodedLgFrame {
                        encrypted: false,
                        cipher_mode: EncryptionMode::Plain,
                        raw_byte_length: 5 + len,
                        json_text: trimmed,
                    });
                }
            } else if first == b'{' {
                if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                    let line = self.buf[..=pos].to_vec();
                    self.buf.drain(0..=pos);
                    if let Ok(text) = String::from_utf8(line) {
                        frames.push(DecodedLgFrame {
                            encrypted: false,
                            cipher_mode: EncryptionMode::Plain,
                            raw_byte_length: text.len(),
                            json_text: text.trim().to_string(),
                        });
                    }
                } else {
                    break;
                }
            } else {
                self.buf.remove(0);
            }
        }
        frames
    }
}

/// Convert Monday(0)..Sunday(6) list to LG 7-bit alarm bitmask: `1 << (6 - day)`
pub fn days_list_to_bitmask(days: &[u8]) -> u8 {
    let mut mask = 0u8;
    for &d in days {
        if d <= 6 {
            mask |= 1 << (6 - d);
        }
    }
    mask
}

pub fn bitmask_to_days_list(mask: u8) -> Vec<u8> {
    let mut out = Vec::new();
    for d in 0..=6 {
        if (mask & (1 << (6 - d))) != 0 {
            out.push(d);
        }
    }
    out
}

// ============================================================================
// RFC 6455 WebSocket SHA-1 + Base64 Handshake
// ============================================================================
pub fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

pub fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn websocket_accept_key(client_key: &str) -> String {
    let mut input = String::with_capacity(client_key.len() + 36);
    input.push_str(client_key.trim());
    input.push_str("258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let digest = sha1_digest(input.as_bytes());
    base64_encode(&digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes256_cbc_and_lg_packet_framer() {
        let mut framer = LgPacketFramer::new();
        let pkt1 = encode_lg_packet(r#"{"msg":"PRODUCT_INFO"}"#, EncryptionMode::Sj8V1);
        let pkt2 = encode_lg_packet(r#"{"msg":"EQ_SETTING","data":{"eq":14}}"#, EncryptionMode::TemescalV2);
        let pkt3 = encode_lg_packet(r#"{"msg":"ALARM_LIST_REQ"}"#, EncryptionMode::Plain);

        let mut combined = Vec::new();
        combined.extend_from_slice(&pkt1);
        combined.extend_from_slice(&pkt2);
        combined.extend_from_slice(&pkt3);

        let first = framer.push(&combined[..11]);
        assert_eq!(first.len(), 0);

        let second = framer.push(&combined[11..]);
        assert_eq!(second.len(), 3);
        assert!(second[0].encrypted);
        assert_eq!(second[0].cipher_mode, EncryptionMode::Sj8V1);
        assert!(second[0].json_text.contains("PRODUCT_INFO"));

        assert!(second[1].encrypted);
        assert_eq!(second[1].cipher_mode, EncryptionMode::TemescalV2);
        assert!(second[1].json_text.contains("EQ_SETTING"));

        assert!(!second[2].encrypted);
        assert_eq!(second[2].cipher_mode, EncryptionMode::Plain);
        assert!(second[2].json_text.contains("ALARM_LIST_REQ"));
    }

    #[test]
    fn test_alarm_weekday_bitmask() {
        assert_eq!(days_list_to_bitmask(&[0, 1]), 96);
        assert_eq!(bitmask_to_days_list(96), vec![0, 1]);
        assert_eq!(days_list_to_bitmask(&[0, 1, 2, 3, 4]), 124);
        assert_eq!(bitmask_to_days_list(124), vec![0, 1, 2, 3, 4]);
    }
}
