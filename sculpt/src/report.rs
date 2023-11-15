use ariadne::{sources, ColorGenerator, Config, Fmt, Label, Report, ReportKind};
use lalrpop_util::ParseError;

use crate::grammar::Token;
use crate::run::Error;

// TODO: Print `identifier` instead of regex string. Might require custom token type?
pub fn report_error(
    file: &std::path::Path,
    source_code: &str,
    error: Error,
    colored: bool,
    writer: impl std::io::Write,
) {
    let file = file.as_os_str().to_str().unwrap().to_string();
    let config = Config::default().with_color(colored);
    let mut colors = ColorGenerator::new();
    let a = colors.next();
    let b = colors.next();
    let fg = |text: String, color| text.to_string().fg(colored.then_some(color));

    let builder = match error {
        Error::MissingFmtStr(range) => Report::build(ReportKind::Error, file.clone(), range.start)
            .with_config(config)
            .with_code("MissingFmtStr")
            .with_label(
                Label::new((file.clone(), range))
                    .with_message("requires at least a format string argument")
                    .with_color(a),
            ),
        Error::ExtraFmtArguments(fmt_str, args) => {
            Report::build(ReportKind::Error, file.clone(), fmt_str.start)
                .with_config(config)
                .with_code("ExtraFmtArguments")
                .with_message(if args.len() == 1 {
                    "unused formatting argument"
                } else {
                    "multiple unused formatting arguments"
                })
                .with_labels(args.into_iter().map(|span| {
                    Label::new((file.clone(), span))
                        .with_message("argument never used")
                        .with_color(a)
                }))
                .with_label(
                    Label::new((file.clone(), fmt_str))
                        .with_message("multiple missing formatting specifiers")
                        .with_color(b),
                )
        }
        Error::NotEnoughFmtArguments(fmt_specifiers, args) => {
            let arguments_a = if fmt_specifiers.len() == 1 {
                "argument"
            } else {
                "arguments"
            };
            let (is_are, arguments_b) = if args.len() == 1 {
                ("is", "argument")
            } else {
                ("are", "arguments")
            };
            Report::build(ReportKind::Error, file.clone(), fmt_specifiers[0].start)
                .with_config(config)
                .with_code("NotEnoughFmtArguments")
                .with_message(format!(
                    "{} positional {} in format string, but there {} {} {}",
                    fmt_specifiers.len(),
                    arguments_a,
                    is_are,
                    args.len(),
                    arguments_b,
                ))
                .with_labels(
                    fmt_specifiers
                        .into_iter()
                        .map(|span| Label::new((file.clone(), span)).with_color(a)),
                )
                .with_labels(
                    args.into_iter()
                        .map(|span| Label::new((file.clone(), span)).with_color(b)),
                )
        }
        Error::ParseError(ParseError::ExtraToken {
            token: (l, Token(_, t), r),
        }) => Report::build(ReportKind::Error, file.clone(), l)
            .with_config(config)
            .with_code("ExtraToken")
            .with_message(format!(
                "encountered unexpected syntax {}",
                fg(format!("\"{}\"", t), a)
            ))
            .with_label(
                Label::new((file.clone(), l..r))
                    .with_message("unexpected syntax")
                    .with_color(a),
            ),
        Error::ParseError(ParseError::InvalidToken { location }) => {
            Report::build(ReportKind::Error, file.clone(), location)
                .with_config(config)
                .with_code("InvalidToken")
                .with_message(format!("encountered unexpected syntax"))
                .with_label(
                    Label::new((file.clone(), location..location + 1))
                        .with_message("unexpected syntax")
                        .with_color(a),
                )
        }
        Error::ParseError(ParseError::UnrecognizedEof { location, expected }) => {
            let expected = expected
                .into_iter()
                .map(|e| format!("{}", fg(e, b)))
                .collect::<Vec<_>>()
                .join(", ");
            Report::build(ReportKind::Error, file.clone(), location)
                .with_config(config)
                .with_code("UnrecognizedEof")
                .with_message(format!("unexpected end of file"))
                .with_label(
                    Label::new((file.clone(), location..location + 1))
                        .with_message(format!("Expected one of: {}", expected))
                        .with_color(b),
                )
        }
        Error::ParseError(ParseError::UnrecognizedToken {
            token: (l, Token(_, t), r),
            expected,
        }) => {
            let expected = expected
                .into_iter()
                .map(|e| format!("{}", fg(e, b)))
                .collect::<Vec<_>>()
                .join(", ");
            let report = Report::build(ReportKind::Error, file.clone(), l)
                .with_config(config)
                .with_code("UnrecognizedToken")
                .with_message(format!(
                    "encountered unexpected syntax {}",
                    fg(format!("\"{}\"", t), a)
                ))
                .with_label(
                    Label::new((file.clone(), l..r))
                        .with_message("unexpected syntax")
                        .with_color(a),
                );
            if !expected.is_empty() {
                report.with_label(
                    Label::new((file.clone(), l..r))
                        .with_message(format!("Expected one of: {}", expected))
                        .with_color(b),
                )
            } else {
                report
            }
        }
        Error::ParseError(error @ ParseError::User { .. }) => unreachable!("{:#?}", error),
    };

    builder
        .finish()
        .write(sources(vec![(file.to_string(), source_code)]), writer)
        .unwrap();
}
