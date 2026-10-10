//! Reading the data files. Every colour, word and tuning value a player meets
//! comes from `data/`; this module turns the JSON into the core's integers.

use crate::fx::ONE;
use serde::Deserialize;
use std::collections::BTreeMap;

/// A colour as three Q16 channels.
pub type Rgb = [i32; 3];

/// `#RRGGBB` to Q16 channels.
pub fn hex(s: &str) -> Result<Rgb, String> {
    let t = s.trim_start_matches('#');
    if t.len() != 6 {
        return Err(format!("not a colour: {s}"));
    }
    let ch = |k: usize| {
        u8::from_str_radix(&t[k..k + 2], 16)
            .map(|v| (v as i32 * ONE) / 255)
            .map_err(|_| format!("not a colour: {s}"))
    };
    Ok([ch(0)?, ch(2)?, ch(4)?])
}

/// The palette: each name holds one colour or a list to choose among.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    colours: BTreeMap<String, Vec<Rgb>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl Palette {
    pub fn parse(json: &str) -> Result<Palette, String> {
        let raw: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(json).map_err(|e| format!("palette.json: {e}"))?;
        let mut colours = BTreeMap::new();
        for (k, v) in raw {
            if k.starts_with('_') {
                continue;
            }
            let list: OneOrMany = serde_json::from_value(v).map_err(|e| format!("palette.json {k}: {e}"))?;
            let list = match list {
                OneOrMany::One(s) => vec![hex(&s)?],
                OneOrMany::Many(v) => v.iter().map(|s| hex(s)).collect::<Result<_, _>>()?,
            };
            colours.insert(k, list);
        }
        Ok(Palette { colours })
    }

    /// The first colour under a name; magenta when the name is missing, so a
    /// gap shows on screen rather than hiding.
    pub fn get(&self, name: &str) -> Rgb {
        self.pick(name, 0)
    }

    /// The `k`th colour under a name, wrapping.
    pub fn pick(&self, name: &str, k: usize) -> Rgb {
        match self.colours.get(name) {
            Some(v) if !v.is_empty() => v[k % v.len()],
            _ => [ONE, 0, ONE],
        }
    }

    pub fn count(&self, name: &str) -> usize {
        self.colours.get(name).map(|v| v.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_reads_each_channel() {
        assert_eq!(hex("#FF0000").unwrap(), [ONE, 0, 0]);
        assert_eq!(hex("00ff80").unwrap()[1], ONE);
        assert!(hex("#12345").is_err());
    }

    #[test]
    fn a_palette_holds_single_colours_and_lists() {
        let p = Palette::parse(r##"{"_about": "x", "ink": "#000000", "walls": ["#FF0000", "#00FF00"]}"##)
            .unwrap();
        assert_eq!(p.get("ink"), [0, 0, 0]);
        assert_eq!(p.pick("walls", 3), [0, ONE, 0]);
        assert_eq!(p.get("missing"), [ONE, 0, ONE]);
        assert_eq!(p.count("walls"), 2);
    }
}
