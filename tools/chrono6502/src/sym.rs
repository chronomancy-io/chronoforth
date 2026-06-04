//! Parse an ACME VICE-label dump (`al C:08e7 .STORE`) into name<->addr maps.

use std::collections::HashMap;

pub struct Symbols {
    pub by_name: HashMap<String, u16>,
}

impl Symbols {
    pub fn load(path: &str) -> std::io::Result<Symbols> {
        let text = std::fs::read_to_string(path)?;
        let mut by_name = HashMap::new();
        for line in text.lines() {
            // format: "al C:08e7 .STORE"
            let mut it = line.split_whitespace();
            if it.next() != Some("al") {
                continue;
            }
            let addr_tok = match it.next() {
                Some(t) => t,
                None => continue,
            };
            let name_tok = match it.next() {
                Some(t) => t,
                None => continue,
            };
            let hex = addr_tok.trim_start_matches("C:").trim_start_matches("c:");
            if let Ok(addr) = u16::from_str_radix(hex, 16) {
                let name = name_tok.trim_start_matches('.').to_string();
                by_name.insert(name, addr);
            }
        }
        Ok(Symbols { by_name })
    }

    pub fn addr(&self, name: &str) -> Option<u16> {
        self.by_name.get(name).copied()
    }
}
