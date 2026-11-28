use selflow::cli::{CliError, error_code, error_text};
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;

#[test]
fn a_version_mismatch_exits_with_code_2_and_names_each_version() {
    let error = CliError::VersionMismatch {
        running: None,
        installed: Some(2),
    };
    assert_eq!(error_code(&error), 2);
    assert_eq!(
        error_text(&Localizer::new(Language::En), &error),
        "The Selflow programs don't match. This command uses protocol 1, the running daemon uses ? \
         and the installed one uses 2. Reinstall Selflow to fix it: make install"
    );
    assert_eq!(
        error_text(&Localizer::new(Language::PtBr), &error),
        "Os programas do Selflow não combinam. Este comando usa o protocolo 1, o daemon em execução \
         usa ? e o instalado usa 2. Reinstale o Selflow para corrigir: make install"
    );
}
