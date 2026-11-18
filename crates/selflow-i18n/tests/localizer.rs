use selflow_i18n::language::Language;
use selflow_i18n::localizer::{Arg, Localizer};

#[test]
fn renders_a_message_with_arguments_in_each_language() {
    let time = || [("time", Arg::Text("07:00".into()))];
    assert_eq!(
        Localizer::new(Language::En).text("lock-until", &time()),
        "Locked until 07:00"
    );
    assert_eq!(
        Localizer::new(Language::PtBr).text("lock-until", &time()),
        "Bloqueado até 07:00"
    );
}

#[test]
fn maps_locale_values_to_a_language() {
    assert_eq!(Language::from_tag("pt_BR.UTF-8"), Some(Language::PtBr));
    assert_eq!(Language::from_tag("en_US.UTF-8"), Some(Language::En));
    assert_eq!(Language::from_tag("de_DE.UTF-8"), None);
    assert_eq!(
        Language::from_env_values(None, Some(""), Some("pt_BR.UTF-8")),
        Language::PtBr
    );
    assert_eq!(
        Language::from_env_values(None, None, Some("de_DE.UTF-8")),
        Language::En
    );
}

#[test]
fn returns_the_id_of_a_missing_message() {
    assert_eq!(
        Localizer::new(Language::En).text("no-such-message", &[]),
        "no-such-message"
    );
}
