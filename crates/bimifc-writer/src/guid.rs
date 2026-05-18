//! IFC GlobalId generation (22-char base64-encoded UUID)

use uuid::Uuid;

/// IFC uses a custom base64 encoding mapping 128-bit UUIDs to 22 characters.
const IFC_BASE64: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// Generate a new IFC-encoded GlobalId string (22 characters).
pub fn new_global_id() -> String {
    encode_uuid(Uuid::new_v4())
}

fn encode_uuid(uuid: Uuid) -> String {
    let b = uuid.as_bytes();
    let mut result = String::with_capacity(22);

    // Pack 16 bytes into 6 groups, encode each group to base-64 chars
    let groups: [u32; 6] = [
        (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32,
        (b[3] as u32) << 16 | (b[4] as u32) << 8 | b[5] as u32,
        (b[6] as u32) << 16 | (b[7] as u32) << 8 | b[8] as u32,
        (b[9] as u32) << 16 | (b[10] as u32) << 8 | b[11] as u32,
        (b[12] as u32) << 16 | (b[13] as u32) << 8 | b[14] as u32,
        b[15] as u32,
    ];

    // First 5 groups: 24 bits → 4 chars each
    for g in &groups[..5] {
        let mut n = *g;
        let mut chars = [0u8; 4];
        for c in chars.iter_mut().rev() {
            *c = IFC_BASE64[(n & 0x3F) as usize];
            n >>= 6;
        }
        for c in &chars {
            result.push(*c as char);
        }
    }

    // Last group: 8 bits → 2 chars
    let n = groups[5];
    result.push(IFC_BASE64[((n >> 6) & 0x3F) as usize] as char);
    result.push(IFC_BASE64[(n & 0x3F) as usize] as char);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_is_22_chars() {
        let guid = new_global_id();
        assert_eq!(guid.len(), 22);
    }

    #[test]
    fn guid_uses_valid_chars() {
        let guid = new_global_id();
        let valid = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";
        for c in guid.chars() {
            assert!(valid.contains(c), "Invalid char in IFC GUID: {c}");
        }
    }

    #[test]
    fn guids_are_unique() {
        let a = new_global_id();
        let b = new_global_id();
        assert_ne!(a, b);
    }
}
