use crate::mac::{format_mac, format_oui, oui_of, parse_mac, parse_oui};
use crate::Error;
use regex_lite::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oui: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mac: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_regex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssid_regex: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub company_id: Vec<u16>,
}

#[derive(Debug, Clone, Default)]
pub struct FilterEngine {
    pub ouis: Vec<[u8; 3]>,
    pub macs: Vec<[u8; 6]>,
    pub name_regex: Option<Regex>,
    pub ssid_regex: Option<Regex>,
    pub company_ids: Vec<u16>,
}

impl PartialEq for FilterEngine {
    fn eq(&self, other: &Self) -> bool {
        self.ouis == other.ouis
            && self.macs == other.macs
            && self.company_ids == other.company_ids
            && regex_pat(&self.name_regex) == regex_pat(&other.name_regex)
            && regex_pat(&self.ssid_regex) == regex_pat(&other.ssid_regex)
    }
}

impl Eq for FilterEngine {}

fn regex_pat(r: &Option<Regex>) -> Option<&str> {
    r.as_ref().map(Regex::as_str)
}

impl FilterEngine {
    pub fn from_spec(spec: &FilterSpec) -> Result<Self, Error> {
        let mut ouis = Vec::new();
        for s in &spec.oui {
            ouis.push(parse_oui(s)?);
        }
        let mut macs = Vec::new();
        for s in &spec.mac {
            macs.push(parse_mac(s)?);
        }
        let name_regex = compile_opt(spec.name_regex.as_deref())?;
        let ssid_regex = compile_opt(spec.ssid_regex.as_deref())?;
        Ok(Self {
            ouis,
            macs,
            name_regex,
            ssid_regex,
            company_ids: spec.company_id.clone(),
        })
    }

    pub fn to_spec(&self) -> FilterSpec {
        FilterSpec {
            oui: self.ouis.iter().map(format_oui).collect(),
            mac: self.macs.iter().map(format_mac).collect(),
            name_regex: self.name_regex.as_ref().map(|r| r.as_str().to_string()),
            ssid_regex: self.ssid_regex.as_ref().map(|r| r.as_str().to_string()),
            company_id: self.company_ids.clone(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.ouis.is_empty()
            && self.macs.is_empty()
            && self.name_regex.is_none()
            && self.ssid_regex.is_none()
            && self.company_ids.is_empty()
    }

    pub fn matches_wifi(&self, mac: &[u8; 6], ssid: Option<&str>) -> bool {
        if !self.ouis.is_empty() && !self.ouis.contains(&oui_of(mac)) {
            return false;
        }
        if !self.macs.is_empty() && !self.macs.contains(mac) {
            return false;
        }
        if let Some(re) = &self.ssid_regex {
            match ssid {
                Some(s) if re.is_match(s) => {}
                _ => return false,
            }
        }
        true
    }

    pub fn matches_ble(&self, mac: &[u8; 6], name: Option<&str>, company_id: Option<u16>) -> bool {
        if !self.ouis.is_empty() && !self.ouis.contains(&oui_of(mac)) {
            return false;
        }
        if !self.macs.is_empty() && !self.macs.contains(mac) {
            return false;
        }
        if let Some(re) = &self.name_regex {
            match name {
                Some(s) if re.is_match(s) => {}
                _ => return false,
            }
        }
        if !self.company_ids.is_empty() {
            match company_id {
                Some(id) if self.company_ids.contains(&id) => {}
                _ => return false,
            }
        }
        true
    }
}

fn compile_opt(pat: Option<&str>) -> Result<Option<Regex>, Error> {
    match pat {
        None | Some("") => Ok(None),
        Some(p) => Regex::new(p)
            .map(Some)
            .map_err(|e| Error::Regex(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_passes_everything() {
        let f = FilterEngine::default();
        assert!(f.is_open());
        assert_eq!(f.to_spec(), FilterSpec::default());
        assert!(f.matches_wifi(&[1, 2, 3, 4, 5, 6], Some("x")));
        assert!(f.matches_ble(&[1, 2, 3, 4, 5, 6], None, None));
    }

    #[test]
    fn oui_mac_regex_company() {
        let spec = FilterSpec {
            oui: vec!["F4:4E:FC".into()],
            mac: vec![],
            name_regex: Some("AirPods.*".into()),
            ssid_regex: Some("^Flock".into()),
            company_id: vec![0x004c],
        };
        let f = FilterEngine::from_spec(&spec).unwrap();
        assert!(!f.is_open());
        assert_eq!(FilterEngine::from_spec(&f.to_spec()).unwrap(), f);
        let apple = [0xf4, 0x4e, 0xfc, 0, 0, 1];
        assert!(f.matches_wifi(&apple, Some("FlockCam")));
        assert!(!f.matches_wifi(&apple, Some("other")));
        assert!(!f.matches_wifi(&[0, 0, 0, 0, 0, 1], Some("Flock")));
        assert!(f.matches_ble(&apple, Some("AirPods Pro"), Some(0x004c)));
        assert!(!f.matches_ble(&apple, Some("Beats"), Some(0x004c)));
        assert!(!f.matches_ble(&apple, Some("AirPods"), Some(1)));
        assert!(!f.matches_ble(&apple, None, Some(0x004c)));
        assert!(!f.matches_ble(&[0, 0, 0, 0, 0, 1], Some("AirPods Pro"), Some(0x004c)));
        let spec = FilterSpec {
            oui: vec![],
            mac: vec!["aa:bb:cc:dd:ee:ff".into()],
            name_regex: None,
            ssid_regex: None,
            company_id: vec![],
        };
        let f = FilterEngine::from_spec(&spec).unwrap();
        assert!(f.matches_wifi(&[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff], None));
        assert!(!f.matches_wifi(&[0xaa, 0xbb, 0xcc, 0, 0, 0], None));
        assert!(f.matches_ble(&[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff], None, None));
        assert!(!f.matches_ble(&[0xaa, 0xbb, 0xcc, 0, 0, 0], None, None));
        assert!(FilterEngine::from_spec(&FilterSpec {
            oui: vec!["bad".into()],
            ..Default::default()
        })
        .is_err());
        assert!(FilterEngine::from_spec(&FilterSpec {
            name_regex: Some("(".into()),
            ..Default::default()
        })
        .is_err());
        let a = FilterEngine::from_spec(&spec).unwrap();
        let b = FilterEngine::from_spec(&spec).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            FilterEngine::from_spec(&FilterSpec {
                name_regex: Some("a".into()),
                ..Default::default()
            })
            .unwrap(),
            FilterEngine::from_spec(&FilterSpec {
                name_regex: Some("a".into()),
                ..Default::default()
            })
            .unwrap()
        );
        assert_ne!(
            FilterEngine::from_spec(&FilterSpec {
                ssid_regex: Some("a".into()),
                ..Default::default()
            })
            .unwrap(),
            FilterEngine::from_spec(&FilterSpec {
                ssid_regex: Some("b".into()),
                ..Default::default()
            })
            .unwrap()
        );
        assert!(FilterEngine::from_spec(&FilterSpec {
            name_regex: Some("".into()),
            ..Default::default()
        })
        .unwrap()
        .name_regex
        .is_none());
        let round = FilterEngine::from_spec(&spec).unwrap();
        assert_eq!(FilterEngine::from_spec(&round.to_spec()).unwrap(), round);
    }
}
