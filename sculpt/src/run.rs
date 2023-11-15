use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::{ExecutionEngine, JitFunction};
use inkwell::module::Module;
use inkwell::values::{FunctionValue, GlobalValue};
use inkwell::{AddressSpace, OptimizationLevel};
use lalrpop_util::ParseError;

use std::io::Write;
use std::ops::Range;

use crate::fmt::{extract_fmt, FmtSpec};
use crate::grammar::{MainParser, Token};
use crate::syntax::{Macro, Main, StrLit};

#[derive(Debug, PartialEq)]
pub enum Error<'src> {
    ParseError(ParseError<usize, Token<'src>, &'src str>),
    MissingFmtStr(Range<usize>),
    ExtraFmtArguments(Range<usize>, Vec<Range<usize>>),
    NotEnoughFmtArguments(Vec<Range<usize>>, Vec<Range<usize>>),
}

pub fn run<'src>(source_code: &'src str, std_out: impl Write) -> Result<(), Error<'src>> {
    let context = &Context::create();
    let module = &context.create_module("main");
    let builder = &context.create_builder();
    let execution_engine = &module
        .create_jit_execution_engine(OptimizationLevel::None)
        .unwrap();
    let mut std_out: Box<dyn Write> = Box::new(std_out);

    let main = build_main(
        context,
        module,
        builder,
        execution_engine,
        source_code,
        &mut std_out,
    )?;

    Ok(unsafe { main.call() })
}

pub fn build_main<'src, 'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    execution_engine: &ExecutionEngine<'ctx>,
    source_code: &'src str,
    std_out: &mut Box<dyn Write + 'ctx>,
) -> Result<JitFunction<'ctx, unsafe extern "C" fn()>, Error<'src>> {
    let Main { statements } = MainParser::new()
        .parse(source_code)
        .map_err(Error::ParseError)?;

    let ext_write = link_write(module, execution_engine);
    let ext_std_out = link_std_out(std_out, module, execution_engine);

    let main_fn = module.add_function("main", context.void_type().fn_type(&[], false), None);
    let main_fn_body = context.append_basic_block(main_fn, "");
    builder.position_at_end(main_fn_body);
    for m in statements {
        build_macro_invocation(m, context, builder, ext_write, ext_std_out)?;
    }
    builder.build_return(None);

    if let Err(e) = module.verify() {
        panic!("{}", e.to_string());
    }
    let main: JitFunction<unsafe extern "C" fn()> =
        unsafe { execution_engine.get_function("main") }.unwrap();
    Ok(main)
}

fn build_macro_invocation<'src>(
    m: Macro<'src>,
    context: &Context,
    builder: &Builder,
    write: FunctionValue,
    std_out: GlobalValue,
) -> Result<(), Error<'src>> {
    let Macro { name, args } = m;
    match name.name {
        "println!" => build_println(context, builder, write, std_out, name.span, args.as_slice()),
        "print!" => build_print(context, builder, write, std_out, name.span, args.as_slice()),
        _ => todo!(),
    }
}

fn build_println<'src>(
    context: &Context,
    builder: &Builder,
    write: FunctionValue,
    std_out: GlobalValue,
    println_name_span: Range<usize>,
    args: &[StrLit<'src>],
) -> Result<(), Error<'src>> {
    if args.len() > 0 {
        build_print(
            context,
            builder,
            write,
            std_out,
            println_name_span.clone(),
            args,
        )?;
    }
    build_print_str(context, builder, write, std_out, "\n");
    Ok(())
}

fn build_print<'src>(
    context: &Context,
    builder: &Builder,
    write: FunctionValue,
    std_out: GlobalValue,
    print_name_span: Range<usize>,
    args: &[StrLit<'src>],
) -> Result<(), Error<'src>> {
    if args.is_empty() {
        return Err(Error::MissingFmtStr(print_name_span.clone()));
    }

    let fmt_str = &args[0];
    let specs = extract_fmt(fmt_str)
        .map_err(|location| Error::ParseError(ParseError::InvalidToken { location }))?;
    let specs = specs.iter();
    let format_specifier_spans: Vec<_> = specs
        .clone()
        .filter_map(|spec| match spec {
            FmtSpec::Arg { span } => Some(span.clone()),
            FmtSpec::Lit { .. } => None,
        })
        .collect();

    let args = &args[1..];
    let expected_arg_count = format_specifier_spans.len();
    if args.len() > expected_arg_count {
        return Err(Error::ExtraFmtArguments(
            fmt_str.span.clone(),
            args[expected_arg_count..]
                .into_iter()
                .map(|arg| arg.span.clone())
                .collect(),
        ));
    }
    if args.len() < expected_arg_count {
        return Err(Error::NotEnoughFmtArguments(
            format_specifier_spans,
            args.into_iter().map(|arg| arg.span.clone()).collect(),
        ));
    }

    let mut args = args.into_iter();
    for spec in specs {
        let lit = match spec {
            FmtSpec::Lit { val, .. } => val,
            FmtSpec::Arg { .. } => args.next().unwrap().val,
        };

        build_print_str(context, builder, write, std_out, lit);
    }
    Ok(())
}

fn build_print_str<'src>(
    context: &Context,
    builder: &Builder,
    write: FunctionValue,
    std_out: GlobalValue,
    lit: &'src str,
) {
    let writer = std_out.as_pointer_value().into();
    let buffer = builder
        .build_global_string_ptr(lit, "")
        .as_pointer_value()
        .into();
    let len = context
        .i64_type()
        .const_int(lit.len().try_into().unwrap(), false)
        .into();
    builder.build_call(write, &[writer, buffer, len], "");
}

fn link_write<'ctx>(
    module: &Module<'ctx>,
    execution_engine: &ExecutionEngine<'ctx>,
) -> FunctionValue<'ctx> {
    let context = module.get_context();
    let i64_type = context.i64_type();
    let i8_type = context.i8_type();
    let box_type = i8_type.ptr_type(AddressSpace::default());

    let ext_write = module.add_function(
        "write",
        i64_type.fn_type(
            &[
                box_type.ptr_type(AddressSpace::default()).into(),
                i8_type.ptr_type(AddressSpace::default()).into(),
                i64_type.into(),
            ],
            false,
        ),
        None,
    );

    extern "C" fn write(os: *mut Box<dyn Write>, s: *const u8, l: u64) -> u64 {
        let os = unsafe { os.as_mut() }.unwrap();
        let s = unsafe { std::slice::from_raw_parts(s, l.try_into().unwrap()) };
        os.write(s).unwrap().try_into().unwrap()
    }

    execution_engine.add_global_mapping(&ext_write, write as usize);
    ext_write
}

fn link_std_out<'ctx>(
    std_out: &mut Box<dyn Write + 'ctx>,
    module: &Module<'ctx>,
    execution_engine: &ExecutionEngine<'ctx>,
) -> GlobalValue<'ctx> {
    let context = module.get_context();
    let box_type = context.i8_type().ptr_type(AddressSpace::default());

    let ext_std_out = module.add_global(box_type, None, "std_out");

    let std_out_ptr = std_out as *mut Box<dyn Write>;
    let std_out_addr = std_out_ptr as usize;

    execution_engine.add_global_mapping(&ext_std_out, std_out_addr);
    ext_std_out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::report_error;
    use textwrap;

    fn dedent(s: &str) -> String {
        textwrap::dedent(s).trim().to_string()
    }

    trait Code {
        fn run<'src>(&'src self) -> Result<String, String>;
    }

    impl Code for str {
        fn run<'src>(&'src self) -> Result<String, String> {
            let mut output_buf = Vec::new();
            let stdout = std::io::BufWriter::new(&mut output_buf);
            run(self, stdout)
                .map(|_| String::from_utf8(output_buf).unwrap())
                .map_err(|error| {
                    let mut error_buf = Vec::new();
                    let stderr = std::io::BufWriter::new(&mut error_buf);
                    report_error(
                        std::path::Path::new("file.sculpt"),
                        self,
                        error,
                        false,
                        stderr,
                    );
                    let error = String::from_utf8(error_buf).unwrap();
                    error
                        .lines()
                        .map(|line| line.trim_end())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
        }
    }

    #[test]
    fn empty_main_works() {
        let src = r#"
            fn main() {
            }
        "#;
        assert_eq!(src.run().unwrap(), "");
    }

    #[test]
    fn hello_world_works() {
        let src = r#"
            fn main() {
                print!("Hello");
                print!(" ");
                print!("world!");
                println!();
            }
        "#;
        assert_eq!(src.run().unwrap(), "Hello world!\n");
    }

    #[test]
    fn str_literals_as_format_args_works() {
        let src = r#"
            fn main() {
                println!("Hello {} and {}!", "Alice", "Bob");
            }
        "#;
        assert_eq!(src.run().unwrap(), "Hello Alice and Bob!\n");
    }

    #[test]
    fn invalid_fmt_string_errors_are_reported() {
        let src = dedent(
            r#"
            fn main() {
                println!("}");
            }
            "#,
        );
        assert_eq!(
            src.run().err().unwrap(),
            dedent(
                r#"
                [InvalidToken] Error: encountered unexpected syntax
                   ╭─[file.sculpt:2:15]
                   │
                 2 │     println!("}");
                   │               ┬
                   │               ╰── unexpected syntax
                ───╯
                "#
            )
        );
    }

    #[test]
    fn missing_fmt_string_errors_are_reported() {
        let src = dedent(
            r#"
            fn main() {
                print!();
            }
            "#,
        );
        assert_eq!(
            src.run().err().unwrap(),
            dedent(
                r#"
                [MissingFmtStr] Error:
                   ╭─[file.sculpt:2:5]
                   │
                 2 │     print!();
                   │     ───┬──
                   │        ╰──── requires at least a format string argument
                ───╯
                "#
            )
        );
    }

    #[test]
    fn extra_fmt_argument_errors_are_reported() {
        let src = dedent(
            r#"
            fn main() {
                print!(" {} ", "a", "b", "c");
            }
            "#,
        );
        assert_eq!(
            src.run().err().unwrap(),
            dedent(
                r#"
                [ExtraFmtArguments] Error: multiple unused formatting arguments
                   ╭─[file.sculpt:2:12]
                   │
                 2 │     print!(" {} ", "a", "b", "c");
                   │            ───┬──       ─┬─  ─┬─
                   │               ╰─────────────────── multiple missing formatting specifiers
                   │                          │    │
                   │                          ╰──────── argument never used
                   │                               │
                   │                               ╰─── argument never used
                ───╯
                "#
            )
        );
    }

    // TODO: Modify labels or trim output before writing so that there's less dead space at the end
    // of the report.
    #[test]
    fn missing_fmt_argument_errors_are_reported() {
        let src = dedent(
            r#"
            fn main() {
                print!("{} {} {}", "a");
            }
            "#,
        );
        assert_eq!(
            src.run().err().unwrap(),
            dedent(
                r#"
                [NotEnoughFmtArguments] Error: 3 positional arguments in format string, but there is 1 argument
                   ╭─[file.sculpt:2:13]
                   │
                 2 │     print!("{} {} {}", "a");
                   │             ── ── ──   ───
                   │
                   │
                   │
                   │
                   │
                   │
                   │
                ───╯
                "#
            )
        );
    }
}
