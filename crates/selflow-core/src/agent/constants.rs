pub const SHELL_OPERATORS: [&str; 12] =
    [";", "&", "|", "`", "$", ">", "<", "(", ")", "{", "}", "\n"];

pub const DENIED_ARGUMENT_PREFIXES: [&str; 1] = ["--output"];

pub const QUOTING: [char; 3] = ['\'', '"', '\\'];

pub const WORD_SEPARATORS: [char; 2] = [' ', '\t'];

pub const HOME_FOLDER: &str = "~";
