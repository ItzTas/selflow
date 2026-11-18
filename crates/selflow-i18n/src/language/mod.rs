#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    En,
    PtBr,
}

impl Language {
    pub fn from_tag(tag: &str) -> Option<Language> {
        let code = tag.split(['-', '_', '.']).next()?;
        if code.eq_ignore_ascii_case("en") {
            return Some(Language::En);
        }
        if code.eq_ignore_ascii_case("pt") {
            return Some(Language::PtBr);
        }
        None
    }

    pub fn tag(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::PtBr => "pt-BR",
        }
    }

    pub fn from_env() -> Language {
        let read = |name: &str| std::env::var(name).ok();
        Language::from_env_values(
            read("LC_ALL").as_deref(),
            read("LC_MESSAGES").as_deref(),
            read("LANG").as_deref(),
        )
    }

    pub fn from_env_values(
        lc_all: Option<&str>,
        lc_messages: Option<&str>,
        lang: Option<&str>,
    ) -> Language {
        [lc_all, lc_messages, lang]
            .into_iter()
            .flatten()
            .find(|value| !value.is_empty())
            .and_then(Language::from_tag)
            .unwrap_or(Language::En)
    }
}
