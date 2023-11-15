use combine::parser::range::recognize;
use combine::Parser;
use combine::{satisfy, skip_many, skip_many1, token};

use std::ops::Range;

use crate::syntax::StrLit;

pub fn extract_fmt<'s>(input: &StrLit<'s>) -> Result<Vec<FmtSpec<'s>>, usize> {
    let lit_parser = || recognize(skip_many1(satisfy(|c| c != '{' && c != '}')));
    let spec_parser = || {
        recognize((
            token('{'),
            skip_many(satisfy(|c| c != '{' && c != '}')),
            token('}'),
        ))
    };

    let mut location = input.span.start + 1;
    let mut input = input.val;
    let mut specs = Vec::new();

    while !input.is_empty() {
        let spec = if let Ok((val, rest)) = lit_parser().parse(input) {
            let span = location..(location + val.len());
            location = span.end;
            input = rest;
            Ok(FmtSpec::Lit { val, span })
        } else if let Ok((spec, rest)) = spec_parser().parse(input) {
            if let Some(offset) =
                spec.find(|c: char| !c.is_ascii_whitespace() && c != '{' && c != '}')
            {
                Err(location + offset)
            } else {
                let span = location..(location + spec.len());
                location = span.end;
                input = rest;
                Ok(FmtSpec::Arg { span })
            }
        } else {
            match input.chars().next().unwrap() {
                '{' => Err(location),
                '}' => Err(location),
                c => unreachable!("{}", c),
            }
        }?;
        specs.push(spec);
    }

    Ok(specs)
}

#[derive(Debug, PartialEq)]
pub enum FmtSpec<'s> {
    Lit { span: Range<usize>, val: &'s str },
    Arg { span: Range<usize> },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str_lit(val: &str) -> StrLit<'_> {
        StrLit {
            span: 0..val.len(),
            val,
        }
    }

    #[test]
    fn literal_extracted_for_plain_str() {
        assert_eq!(
            extract_fmt(&str_lit("abc")).unwrap(),
            [FmtSpec::Lit {
                span: 1..4,
                val: "abc"
            }]
        );
    }

    #[test]
    fn arg_extracted_for_only_arg_str() {
        assert_eq!(
            extract_fmt(&str_lit("{}")).unwrap(),
            [FmtSpec::Arg { span: 1..3 }]
        );
    }

    #[test]
    fn arg_extracted_for_only_arg_str_with_space_in_middle() {
        assert_eq!(
            extract_fmt(&str_lit("{  }")).unwrap(),
            [FmtSpec::Arg { span: 1..5 }]
        );
    }

    #[test]
    fn error_on_unexpected_close_in_first_chunk() {
        assert_eq!(extract_fmt(&str_lit("abc} {} ")).unwrap_err(), 4);
    }

    #[test]
    fn error_on_unexpected_close_in_last_chunk() {
        assert_eq!(extract_fmt(&str_lit("{} {} abc}")).unwrap_err(), 10);
    }

    #[test]
    fn error_when_extracting_unclosed_arg() {
        assert_eq!(extract_fmt(&str_lit("abc{  ")).unwrap_err(), 4);
    }

    #[test]
    fn error_when_extracting_arg_with_non_whitespace_chars() {
        assert_eq!(extract_fmt(&str_lit("abc{ a 1 ; }")).unwrap_err(), 6);
    }

    #[test]
    fn arg_and_lit_extracted_when_arg_at_beginning_of_str() {
        assert_eq!(
            extract_fmt(&str_lit("{} abc")).unwrap(),
            [
                FmtSpec::Arg { span: 1..3 },
                FmtSpec::Lit {
                    span: 3..7,
                    val: " abc"
                }
            ]
        );
    }

    #[test]
    fn lit_and_arg_and_lit_extracted_when_arg_in_middle_of_str() {
        assert_eq!(
            extract_fmt(&str_lit("abc {} def")).unwrap(),
            [
                FmtSpec::Lit {
                    span: 1..5,
                    val: "abc "
                },
                FmtSpec::Arg { span: 5..7 },
                FmtSpec::Lit {
                    span: 7..11,
                    val: " def"
                }
            ]
        );
    }

    #[test]
    fn lit_and_arg_extracted_when_arg_at_end_of_str() {
        assert_eq!(
            extract_fmt(&str_lit("abc {}")).unwrap(),
            [
                FmtSpec::Lit {
                    span: 1..5,
                    val: "abc "
                },
                FmtSpec::Arg { span: 5..7 },
            ]
        );
    }

    #[test]
    fn two_args_extracted_when_two_args_are_adjacent_in_str() {
        assert_eq!(
            extract_fmt(&str_lit("{}{}")).unwrap(),
            [FmtSpec::Arg { span: 1..3 }, FmtSpec::Arg { span: 3..5 },]
        );
    }
}
