mod types;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

use crate::catalog::catalog;
use crate::language::Language;

pub use types::Arg;

pub struct Localizer {
    language: Language,
    bundle: FluentBundle<FluentResource>,
}

impl Localizer {
    pub fn new(language: Language) -> Localizer {
        let identifier: LanguageIdentifier = language
            .tag()
            .parse()
            .expect("language tags are valid identifiers");
        let mut bundle = FluentBundle::new_concurrent(vec![identifier]);
        bundle.set_use_isolating(false);
        for (file, source) in catalog(language) {
            let resource = match FluentResource::try_new((*source).to_owned()) {
                Ok(resource) => resource,
                Err((resource, errors)) => {
                    tracing::warn!(file, ?errors, "translation file has syntax errors");
                    resource
                }
            };
            if let Err(errors) = bundle.add_resource(resource) {
                tracing::warn!(file, ?errors, "translation file has overriding messages");
            }
        }
        Localizer { language, bundle }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    pub fn text(&self, id: &str, args: &[(&str, Arg)]) -> String {
        let Some(pattern) = self
            .bundle
            .get_message(id)
            .and_then(|message| message.value())
        else {
            tracing::warn!(id, "missing translation");
            return id.to_owned();
        };
        let mut fluent_args = FluentArgs::new();
        for (name, arg) in args {
            fluent_args.set(*name, fluent_value(arg));
        }
        let mut errors = Vec::new();
        let rendered = self
            .bundle
            .format_pattern(pattern, Some(&fluent_args), &mut errors);
        if !errors.is_empty() {
            tracing::warn!(id, ?errors, "translation rendered with errors");
        }
        rendered.into_owned()
    }
}

fn fluent_value(arg: &Arg) -> FluentValue<'static> {
    match arg {
        Arg::Text(text) => FluentValue::from(text.clone()),
        Arg::Number(number) => FluentValue::from(*number),
    }
}
