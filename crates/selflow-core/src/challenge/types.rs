use jiff::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionAnswer {
    pub reason: String,
    pub phrase: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    TooEarly { available_at: Timestamp },
    NoBudget,
    ReasonTooShort { min_chars: usize },
    PhraseMismatch,
}
