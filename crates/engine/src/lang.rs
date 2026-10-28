//! The language of messages shown to the user. Data written into a project stays as it was
//! written: a new project takes its default names in the language it was created in
//! (`Document::new_in`), and replaying a file gives the same project wherever it is opened.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    /// Vietnamese, the app's original language.
    #[default]
    Vi,
    En,
    Ja,
}

impl Lang {
    /// The language for a code such as `"vi"`, `"en"`, or `"ja"`.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "vi" => Some(Self::Vi),
            "en" => Some(Self::En),
            "ja" => Some(Self::Ja),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Vi => "vi",
            Self::En => "en",
            Self::Ja => "ja",
        }
    }

    /// Pick between Vietnamese and English (falling back to English for Japanese).
    pub fn pick<T>(self, vi: T, en: T) -> T {
        match self {
            Self::Vi => vi,
            Self::En | Self::Ja => en,
        }
    }

    /// Pick the text for this language among Vietnamese, English, and Japanese.
    pub fn pick3<T>(self, vi: T, en: T, ja: T) -> T {
        match self {
            Self::Vi => vi,
            Self::En => en,
            Self::Ja => ja,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_match_serde() {
        for lang in [Lang::Vi, Lang::En, Lang::Ja] {
            assert_eq!(Lang::from_code(lang.code()), Some(lang));
            assert_eq!(
                serde_json::to_string(&lang).unwrap(),
                format!("\"{}\"", lang.code())
            );
        }
        assert_eq!(Lang::from_code("fr"), None);
        assert_eq!(Lang::default(), Lang::Vi);
    }
}
