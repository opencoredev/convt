//! Primary keys: a type prefix and 128 random bits in lowercase Crockford base32,
//! the same spec as `packages/license/src/ids.ts`. Both check
//! `packages/license/vectors/ids.json`.

const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

pub fn encode_id128(bytes: [u8; 16]) -> String {
    let mut n = u128::from_be_bytes(bytes);
    let mut out = [0u8; 26];
    for slot in out.iter_mut().rev() {
        *slot = ALPHABET[(n & 31) as usize];
        n >>= 5;
    }
    String::from_utf8(out.to_vec()).expect("the alphabet is ASCII")
}

pub fn new_id(prefix: &str) -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the OS random source works");
    format!("{prefix}_{}", encode_id128(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectors() {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../../../packages/license/vectors/ids.json"))
                .unwrap();
        for case in v["ids"].as_array().unwrap() {
            let bytes: [u8; 16] = hex::decode(case["bytes_hex"].as_str().unwrap())
                .unwrap()
                .try_into()
                .unwrap();
            assert_eq!(encode_id128(bytes), case["encoded"], "{}", case["name"]);
        }
    }

    #[test]
    fn new_ids_have_the_prefix() {
        let id = new_id("use");
        assert!(id.starts_with("use_") && id.len() == 30, "{id}");
        assert_ne!(id, new_id("use"));
    }
}
