use crate::Error;
use espcap_protocol::mac::format_oui;
use espcap_protocol::FilterSpec;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct OuiEntry {
    pub oui: [u8; 3],
    pub name: String,
}

pub fn load_oui_csv(csv: &str) -> Result<Vec<OuiEntry>, Error> {
    let mut out = Vec::new();
    for (i, line) in csv.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || (i == 0 && line.to_ascii_lowercase().starts_with("oui")) {
            continue;
        }
        let Some((oui_s, name)) = line.split_once(',') else {
            return Err(Error::msg(format!("bad oui csv line: {line}")));
        };
        out.push(OuiEntry {
            oui: espcap_protocol::mac::parse_oui(oui_s.trim())?,
            name: name.trim().to_string(),
        });
    }
    Ok(out)
}

pub fn ouis_for_manufacturer(db: &[OuiEntry], pattern: &str) -> Result<Vec<[u8; 3]>, Error> {
    let re = Regex::new(pattern)?;
    let mut found: Vec<[u8; 3]> = db
        .iter()
        .filter(|e| re.is_match(&e.name))
        .map(|e| e.oui)
        .collect();
    found.sort();
    found.dedup();
    if found.is_empty() {
        return Err(Error::msg(format!(
            "no OUI matched manufacturer regex {pattern}"
        )));
    }
    Ok(found)
}

pub fn builtin_oui_db() -> Result<Vec<OuiEntry>, Error> {
    load_oui_csv(include_str!("../data/oui.csv"))
}

pub fn merge_filters(
    oui: Option<Vec<String>>,
    mac: Option<Vec<String>>,
    name_regex: Option<String>,
    ssid_regex: Option<String>,
    company_id: Option<Vec<u16>>,
    manufacturer_regex: Option<&str>,
    db: &[OuiEntry],
) -> Result<Option<FilterSpec>, Error> {
    let mut spec = FilterSpec::default();
    if let Some(v) = oui {
        spec.oui = v;
    }
    if let Some(pat) = manufacturer_regex {
        for o in ouis_for_manufacturer(db, pat)? {
            spec.oui
                .push(format_oui(&o).replace(':', "").to_ascii_uppercase());
        }
    }
    if let Some(v) = mac {
        spec.mac = v;
    }
    spec.name_regex = name_regex;
    spec.ssid_regex = ssid_regex;
    if let Some(v) = company_id {
        spec.company_id = v;
    }
    spec.oui.sort();
    spec.oui.dedup();
    if spec.oui.is_empty()
        && spec.mac.is_empty()
        && spec.name_regex.is_none()
        && spec.ssid_regex.is_none()
        && spec.company_id.is_empty()
    {
        Ok(None)
    } else {
        Ok(Some(spec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_and_match_apple() {
        let db = builtin_oui_db().unwrap();
        let ouis = ouis_for_manufacturer(&db, "Apple").unwrap();
        assert!(ouis.iter().any(|o| format_oui(o) == "f4:4e:fc"));
        assert!(ouis_for_manufacturer(&db, "NoSuchVendor").is_err());
        assert!(load_oui_csv("oui,name\nbadline\n").is_err());
        assert!(load_oui_csv("").unwrap().is_empty());
        assert!(ouis_for_manufacturer(&db, "(").is_err());
        let ouis = ouis_for_manufacturer(&db, "Espressif").unwrap();
        assert!(ouis.len() > 1);
        assert!(merge_filters(None, None, None, None, None, None, &db)
            .unwrap()
            .is_none());
        let spec = merge_filters(
            Some(vec!["aabbcc".into()]),
            Some(vec!["aa:bb:cc:dd:ee:ff".into()]),
            Some("n.*".into()),
            Some("s.*".into()),
            Some(vec![76]),
            Some("Apple"),
            &db,
        )
        .unwrap()
        .unwrap();
        assert!(spec.oui.len() > 1);
        assert_eq!(spec.company_id, vec![76]);
    }
}
