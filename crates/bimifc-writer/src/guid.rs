//! IFC GlobalId generation (22-char base64-encoded UUID)

use uuid::Uuid;

/// IFC uses a custom base64 encoding mapping 128-bit UUIDs to 22 characters.
const IFC_BASE64: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// Generate a new IFC-encoded GlobalId string (22 characters).
pub fn new_global_id() -> String {
    encode_uuid(Uuid::new_v4())
}

/// Deterministic GlobalId for a stable source identifier.
///
/// A RFC 4122 UUID string (e.g. RoomPlan's surface/object identifier) is
/// encoded directly, so the IFC GlobalId is the compressed form of that
/// UUID. Any other string is hashed into a name-based (v5) UUID first.
/// Re-exporting the same scan therefore yields the same GlobalIds.
pub fn global_id_from_identifier(identifier: &str) -> String {
    let uuid = Uuid::parse_str(identifier.trim())
        .unwrap_or_else(|_| Uuid::new_v5(&Uuid::NAMESPACE_OID, identifier.as_bytes()));
    encode_uuid(uuid)
}

/// Deterministic GlobalId for an entity *derived* from another one, e.g.
/// the opening cut for a door, or a type object named after its key.
pub fn derived_global_id(base: &str, role: &str) -> String {
    let name = format!("{role}:{base}");
    encode_uuid(Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes()))
}

/// buildingSMART GlobalId compression: the *first* byte alone becomes two
/// characters (2 + 6 bits), then five 3-byte groups become four characters
/// each. Because the leading character carries only two bits it is always
/// `0`–`3`; validators (IfcOpenShell, Solibri, BIMcollab) check exactly that.
fn encode_uuid(uuid: Uuid) -> String {
    let b = uuid.as_bytes();
    let mut result = String::with_capacity(22);

    // First group: 8 bits → 2 chars
    let n = b[0] as u32;
    result.push(IFC_BASE64[((n >> 6) & 0x3F) as usize] as char);
    result.push(IFC_BASE64[(n & 0x3F) as usize] as char);

    // Remaining 5 groups: 24 bits → 4 chars each
    for chunk in b[1..].as_chunks::<3>().0 {
        let mut n = (chunk[0] as u32) << 16 | (chunk[1] as u32) << 8 | chunk[2] as u32;
        let mut chars = [0u8; 4];
        for c in chars.iter_mut().rev() {
            *c = IFC_BASE64[(n & 0x3F) as usize];
            n >>= 6;
        }
        for c in &chars {
            result.push(*c as char);
        }
    }

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
    fn identifier_guids_are_stable() {
        let a = global_id_from_identifier("6BA7B810-9DAD-11D1-80B4-00C04FD430C8");
        let b = global_id_from_identifier("6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        assert_eq!(a, b);
        assert_eq!(a.len(), 22);
        // Non-UUID strings are hashed, still deterministic.
        assert_eq!(
            global_id_from_identifier("wall-7"),
            global_id_from_identifier("wall-7")
        );
        assert_ne!(
            global_id_from_identifier("wall-7"),
            global_id_from_identifier("wall-8")
        );
        // Derived ids differ per role but are stable.
        assert_eq!(
            derived_global_id("x", "opening"),
            derived_global_id("x", "opening")
        );
        assert_ne!(
            derived_global_id("x", "opening"),
            derived_global_id("x", "slab")
        );
    }

    #[test]
    fn guid_first_char_is_0_to_3() {
        for _ in 0..256 {
            let guid = new_global_id();
            let first = guid.chars().next().unwrap();
            assert!(
                ('0'..='3').contains(&first),
                "GlobalId must start with 0-3, got {guid}"
            );
        }
    }

    #[test]
    fn guid_matches_reference_vectors() {
        // Known compressions from the buildingSMART reference implementation.
        assert_eq!(encode_uuid(Uuid::nil()), "0000000000000000000000");
        assert_eq!(encode_uuid(Uuid::max()), "3$$$$$$$$$$$$$$$$$$$$$");
        // First byte 0x40 → "10"; the rest zero.
        let mut bytes = [0u8; 16];
        bytes[0] = 0x40;
        assert_eq!(
            encode_uuid(Uuid::from_bytes(bytes)),
            "1000000000000000000000"
        );
    }

    #[test]
    fn guids_are_unique() {
        let a = new_global_id();
        let b = new_global_id();
        assert_ne!(a, b);
    }
}
