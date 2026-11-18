use crate::language::Language;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/catalog.rs"));
}

pub fn catalog(language: Language) -> &'static [(&'static str, &'static str)] {
    match language {
        Language::En => generated::EN,
        Language::PtBr => generated::PT_BR,
    }
}

pub fn default_config(language: Language) -> &'static str {
    match language {
        Language::En => include_str!("../../../../locales/en/default-config.toml"),
        Language::PtBr => include_str!("../../../../locales/pt-BR/default-config.toml"),
    }
}
