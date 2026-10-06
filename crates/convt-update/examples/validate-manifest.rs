//! Validate unsigned website JSON using exactly the client's schema rules.
fn main() {
    let path = std::env::args().nth(1).expect("manifest path");
    let m: convt_update::Manifest =
        serde_json::from_slice(&std::fs::read(path).unwrap()).expect("manifest schema");
    m.validate().expect("manifest semantic schema");
    println!("PASS website manifest schema");
}
