use std::fmt::Display;

use owo_colors::{OwoColorize, Stream};

pub fn strong(value: impl Display) -> String {
    value
        .if_supports_color(Stream::Stdout, |value| value.bold())
        .to_string()
}

pub fn muted(value: impl Display) -> String {
    value
        .if_supports_color(Stream::Stdout, |value| value.dimmed())
        .to_string()
}

pub fn success(value: impl Display) -> String {
    value
        .if_supports_color(Stream::Stdout, |value| value.green())
        .to_string()
}

pub fn warning(value: impl Display) -> String {
    value
        .if_supports_color(Stream::Stdout, |value| value.yellow())
        .to_string()
}

pub fn failure(value: impl Display) -> String {
    value
        .if_supports_color(Stream::Stdout, |value| value.red())
        .to_string()
}

pub fn stderr_success(value: impl Display) -> String {
    value
        .if_supports_color(Stream::Stderr, |value| value.green())
        .to_string()
}

pub fn stderr_muted(value: impl Display) -> String {
    value
        .if_supports_color(Stream::Stderr, |value| value.dimmed())
        .to_string()
}

pub fn stderr_failure(value: impl Display) -> String {
    value
        .if_supports_color(Stream::Stderr, |value| value.red())
        .to_string()
}
