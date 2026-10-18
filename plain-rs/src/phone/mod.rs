mod data;
mod locale;

use locale::Locale;
use rlibphonenumber::{PHONE_NUMBER_UTIL as util, PhoneNumber, PhoneNumberType as Type};
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub country: String,
    pub number_type: String,
    pub carrier: String,
    pub description: String,
}

pub fn default_region<'a>(network: &'a str, sim: &'a str, locale: &'a str) -> &'a str {
    [network, sim, locale]
        .into_iter()
        .find(|region| !region.trim().is_empty())
        .unwrap_or_default()
}

#[derive(Default)]
pub struct Service {
    data: data::Data,
    cache: Mutex<VecDeque<(String, Option<Metadata>)>>,
    #[cfg(test)]
    resolutions: std::sync::atomic::AtomicUsize,
}

impl Service {
    pub fn lookup(&self, number: &str, region: &str, language_tag: &str) -> Option<Metadata> {
        if number.trim().is_empty() {
            return None;
        }
        let key = format!("{region}\0{language_tag}\0{number}");
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(index) = cache.iter().position(|(cached, _)| *cached == key) {
                let entry = cache.remove(index).unwrap();
                let result = entry.1.clone();
                cache.push_back(entry);
                return result;
            }
        }
        let result = self.resolve(number, region, language_tag);
        if key.len() <= 4096 {
            let mut cache = self.cache.lock().unwrap();
            cache.retain(|(cached, _)| *cached != key);
            cache.push_back((key, result.clone()));
            while cache.len() > 1024 {
                cache.pop_front();
            }
        }
        result
    }

    fn resolve(&self, number: &str, region: &str, language_tag: &str) -> Option<Metadata> {
        #[cfg(test)]
        self.resolutions
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let parsed = util
            .parse(number, region.to_ascii_uppercase().parse().ok())
            .ok()?;
        if !util.is_valid_number(&parsed) {
            return None;
        }
        let country = util
            .get_region_for_country_code(parsed.country_code)?
            .to_string();
        let number_type = util.get_number_type(&parsed);
        let locale = Locale::parse(language_tag);
        let carrier = if matches!(
            number_type,
            Type::Mobile | Type::FixedLineOrMobile | Type::Pager
        ) {
            self.prefix("carrier", &parsed, &locale)
        } else {
            String::new()
        };
        let geographical = self.geographical_number(&parsed);
        let description = self.prefix("geo", geographical.as_ref().unwrap_or(&parsed), &locale);
        let description = if description.is_empty() {
            self.country_name(&parsed, &locale)
        } else {
            description
        };
        Some(Metadata {
            country,
            number_type: type_name(number_type).into(),
            carrier,
            description,
        })
    }

    fn geographical_number(&self, number: &PhoneNumber) -> Option<PhoneNumber> {
        let token = util.get_country_mobile_token(number.country_code)?;
        let national = rlibphonenumber::get_national_significant_number_owned(number);
        let national = national.strip_prefix(token)?;
        util.parse(
            national,
            util.get_region_for_country_code(number.country_code),
        )
        .ok()
    }

    fn prefix(&self, kind: &str, number: &PhoneNumber, locale: &Locale) -> String {
        let key = if number.country_code == 1 {
            1000 + (number.national_number / 10_000_000) as i32
        } else {
            number.country_code
        };
        let digits = format!(
            "{}{}",
            number.country_code,
            rlibphonenumber::get_national_significant_number_owned(number)
        );
        let Ok(digits) = digits.parse() else {
            return String::new();
        };
        let lookup = |language: &str| {
            self.data
                .get(&format!("{kind}/{key}_{language}"))
                .map(|map| map.prefix(digits).to_owned())
                .unwrap_or_default()
        };
        let normalized = (locale.language == "zh"
            && matches!(locale.region.as_str(), "TW" | "HK" | "MO"))
        .then_some("zh_Hant");
        let mut candidates = Vec::new();
        if let Some(language) = normalized {
            candidates.push(language.to_owned());
        }
        if !locale.language.is_empty() {
            if !locale.region.is_empty() {
                candidates.push(format!("{}_{}", locale.language, locale.region));
            }
            candidates.push(locale.language.clone());
        }
        let description = candidates
            .iter()
            .find(|language| self.data.contains(&format!("{kind}/{key}_{language}")))
            .map(|language| lookup(language))
            .unwrap_or_default();
        if !description.is_empty() || matches!(locale.language.as_str(), "zh" | "ja" | "ko") {
            return description;
        }
        lookup("en")
    }

    fn country_name(&self, number: &PhoneNumber, locale: &Locale) -> String {
        let Some(regions) = util.get_regions_for_country_calling_code(number.country_code) else {
            return String::new();
        };
        let mut valid = regions.filter(|region| util.is_valid_number_for_region(number, *region));
        let Some(region) = valid.next() else {
            return String::new();
        };
        if valid.next().is_some() {
            return String::new();
        }
        let region = region.to_string();
        if matches!(region.as_str(), "001" | "ZZ") {
            return String::new();
        }
        for tag in locale.country_tags() {
            if let Some(map) = self.data.get(&format!("countries/{tag}")) {
                return map.country(&region).to_owned();
            }
        }
        region
    }
}

fn type_name(kind: Type) -> &'static str {
    match kind {
        Type::FixedLine => "FIXED_LINE",
        Type::Mobile => "MOBILE",
        Type::FixedLineOrMobile => "FIXED_LINE_OR_MOBILE",
        Type::TollFree => "TOLL_FREE",
        Type::PremiumRate => "PREMIUM_RATE",
        Type::SharedCost => "SHARED_COST",
        Type::PersonalNumber => "PERSONAL_NUMBER",
        Type::VoIP => "VOIP",
        Type::Pager => "PAGER",
        Type::UAN => "UAN",
        Type::VoiceMail => "VOICEMAIL",
        _ => "",
    }
}

#[cfg(test)]
#[path = "../../tests/unit/phone/mod.rs"]
mod tests;
