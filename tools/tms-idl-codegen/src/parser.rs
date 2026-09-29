//! Recursive-descent nom parser for the annotated OMG-IDL subset MIL-STD-3071 uses. Handles nested
//! `module`, `struct`/`enum`/`typedef`/`const`, `sequence<T,N>`/`string<N>`/`char[N]`, member and
//! type annotations, line + block comments, and `#define`. Line numbers are tracked with
//! `nom_locate`. Declaration-order-independent forward refs are resolved later (two-pass) in
//! `resolve.rs`; this pass is purely syntactic.

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until, take_while};
use nom::character::complete::{char, digit1, multispace1, satisfy};
use nom::combinator::{map, opt, recognize, value};
use nom::multi::{many0, separated_list0};
use nom::sequence::{delimited, pair, preceded};
use nom::IResult;
use nom_locate::{position, LocatedSpan};
use std::collections::HashMap;

use crate::ast::*;

pub type Span<'a> = LocatedSpan<&'a str>;

/// Result of preprocessing: the comment-preserving source (with `#`-directive lines blanked so line
/// numbers stay true) plus the `#define` macro table.
pub struct Preprocessed {
    pub source: String,
    pub defines: HashMap<String, f64>,
}

/// Strip C-preprocessor directive lines (`#define …`), capturing numeric `#define`s, and blank the
/// line (preserving the newline) so every later line number matches the original file.
pub fn preprocess(input: &str) -> Preprocessed {
    let mut defines = HashMap::new();
    let mut out = String::with_capacity(input.len());
    for line in input.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            // e.g. `#define RADIAN_MAX 3.1416`
            let body = trimmed.trim_start_matches('#').trim_start();
            if let Some(rest) = body.strip_prefix("define") {
                let mut it = rest.split_whitespace();
                if let (Some(name), Some(val)) = (it.next(), it.next()) {
                    if let Ok(v) = val.parse::<f64>() {
                        defines.insert(name.to_string(), v);
                    }
                }
            }
            // keep only the trailing newline(s) so line numbering is preserved
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    Preprocessed {
        source: out,
        defines,
    }
}

/// Parse a preprocessed IDL source into an [`Idl`]. Errors if any input is left unconsumed (a
/// construct we don't handle) — that is the "parse the FULL IDL clean" guarantee.
pub fn parse_idl(source: &str) -> Result<Idl, String> {
    let span = Span::new(source);
    let (rest, items) =
        preceded(ws0, many0(item))(span).map_err(|e| format!("parse error: {e}"))?;
    let (rest, _) = ws0(rest).map_err(|e: nom::Err<nom::error::Error<Span>>| format!("{e}"))?;
    if !rest.fragment().is_empty() {
        let line = rest.location_line();
        let snippet: String = rest.fragment().chars().take(80).collect();
        return Err(format!(
            "unparsed content at line {line} (a construct the grammar does not handle):\n  {snippet}"
        ));
    }
    Ok(Idl { items })
}

// ---- whitespace / comments -------------------------------------------------------------------

fn line_comment(i: Span) -> IResult<Span, ()> {
    value((), pair(tag("//"), take_while(|c| c != '\n')))(i)
}

fn block_comment(i: Span) -> IResult<Span, ()> {
    value((), delimited(tag("/*"), take_until("*/"), tag("*/")))(i)
}

/// Zero or more whitespace / line-comment / block-comment tokens.
fn ws0(i: Span) -> IResult<Span, ()> {
    value(
        (),
        many0(alt((value((), multispace1), line_comment, block_comment))),
    )(i)
}

/// A literal `s`, skipping leading whitespace/comments.
fn sym<'a>(s: &'static str) -> impl FnMut(Span<'a>) -> IResult<Span<'a>, Span<'a>> {
    move |i| preceded(ws0, tag(s))(i)
}

// ---- identifiers / literals ------------------------------------------------------------------

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn is_ident_cont(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn simple_ident(i: Span) -> IResult<Span, Span> {
    recognize(pair(satisfy(is_ident_start), take_while(is_ident_cont)))(i)
}

/// A possibly `::`-qualified identifier (`tms::ac::MeasurementUpdate`), leading ws skipped.
fn qualified_ident(i: Span) -> IResult<Span, String> {
    map(
        preceded(
            ws0,
            recognize(pair(simple_ident, many0(pair(tag("::"), simple_ident)))),
        ),
        |s: Span| s.fragment().to_string(),
    )(i)
}

fn ident(i: Span) -> IResult<Span, String> {
    map(preceded(ws0, simple_ident), |s: Span| {
        s.fragment().to_string()
    })(i)
}

fn integer(i: Span) -> IResult<Span, i64> {
    map(
        preceded(ws0, recognize(pair(opt(char('-')), digit1))),
        |s: Span| s.fragment().parse::<i64>().unwrap_or(0),
    )(i)
}

fn float_lit(i: Span) -> IResult<Span, f64> {
    map(
        preceded(
            ws0,
            recognize(pair(opt(char('-')), pair(digit1, pair(char('.'), digit1)))),
        ),
        |s: Span| s.fragment().parse::<f64>().unwrap_or(0.0),
    )(i)
}

fn string_lit(i: Span) -> IResult<Span, String> {
    map(
        preceded(
            ws0,
            delimited(char('"'), take_while(|c| c != '"'), char('"')),
        ),
        |s: Span| s.fragment().to_string(),
    )(i)
}

// ---- annotations -----------------------------------------------------------------------------

fn annotation(i: Span) -> IResult<Span, Ann> {
    let (i, _) = ws0(i)?;
    let (i, _) = char('@')(i)?;
    let (i, name) = map(simple_ident, |s: Span| s.fragment().to_string())(i)?;
    let (i, args) = opt(delimited(char('('), take_while(|c| c != ')'), char(')')))(i)?;
    let args = args.map(|s: Span| s.fragment().to_string());
    Ok((i, interpret_ann(&name, args.as_deref())))
}

fn interpret_ann(name: &str, args: Option<&str>) -> Ann {
    match name {
        "topic" => Ann::Topic,
        "nested" => Ann::Nested,
        "key" => Ann::Key,
        "optional" => Ann::Optional,
        "extensibility" => Ann::Extensibility(args.unwrap_or("").trim().to_string()),
        "unit" => Ann::Unit(args.unwrap_or("").trim().trim_matches('"').to_string()),
        "range" => {
            let (lo, hi) = parse_range_args(args.unwrap_or(""));
            Ann::Range(lo, hi)
        }
        "min" => Ann::Min(parse_rangeval(args.unwrap_or("").trim())),
        "max" => Ann::Max(parse_rangeval(args.unwrap_or("").trim())),
        other => Ann::Other(other.to_string()),
    }
}

/// Parse `min=<val>, max=<val>` (order-independent) into `(min, max)`.
fn parse_range_args(s: &str) -> (RangeVal, RangeVal) {
    let mut lo = RangeVal::Num(f64::NEG_INFINITY);
    let mut hi = RangeVal::Num(f64::INFINITY);
    for part in s.split(',') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("min=") {
            lo = parse_rangeval(v.trim());
        } else if let Some(v) = part.strip_prefix("max=") {
            hi = parse_rangeval(v.trim());
        }
    }
    (lo, hi)
}

fn parse_rangeval(s: &str) -> RangeVal {
    let s = s.trim();
    if let Ok(n) = s.parse::<f64>() {
        return RangeVal::Num(n);
    }
    let (negated, name) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, s),
    };
    RangeVal::Ident {
        name: name.to_string(),
        negated,
    }
}

// ---- type references -------------------------------------------------------------------------

fn bound(i: Span) -> IResult<Span, Bound> {
    alt((
        map(integer, |n| Bound::Lit(n.max(0) as u64)),
        map(qualified_ident, Bound::Ident),
    ))(i)
}

fn type_ref(i: Span) -> IResult<Span, TypeRef> {
    alt((sequence_type, bounded_string_type, named_type))(i)
}

fn sequence_type(i: Span) -> IResult<Span, TypeRef> {
    let (i, _) = sym("sequence")(i)?;
    let (i, _) = sym("<")(i)?;
    let (i, elem) = type_ref(i)?;
    let (i, _) = sym(",")(i)?;
    let (i, b) = bound(i)?;
    let (i, _) = sym(">")(i)?;
    Ok((i, TypeRef::Sequence(Box::new(elem), b)))
}

fn bounded_string_type(i: Span) -> IResult<Span, TypeRef> {
    let (i, _) = sym("string")(i)?;
    let (i, _) = sym("<")(i)?;
    let (i, first) = bound(i)?;
    let (i, second) = opt(preceded(sym(","), bound))(i)?;
    let (i, _) = sym(">")(i)?;
    Ok((i, TypeRef::BoundedString(second.unwrap_or(first))))
}

/// A primitive keyword or a named/qualified type. Must NOT swallow the `sequence`/`string` keywords
/// (tried first in [`type_ref`]).
fn named_type(i: Span) -> IResult<Span, TypeRef> {
    map(qualified_ident, TypeRef::Named)(i)
}

// ---- items -----------------------------------------------------------------------------------

fn item(i: Span) -> IResult<Span, Item> {
    alt((
        map(module, Item::Module),
        map(typedef_def, Item::Typedef),
        map(const_def, Item::Const),
        map(enum_def, Item::Enum),
        map(struct_def, Item::Struct),
    ))(i)
}

fn located_keyword<'a>(kw: &'static str) -> impl FnMut(Span<'a>) -> IResult<Span<'a>, u32> {
    move |i| {
        let (i, _) = ws0(i)?;
        let (i, pos) = position(i)?;
        let (i, _) = tag(kw)(i)?;
        Ok((i, pos.location_line()))
    }
}

fn module(i: Span) -> IResult<Span, Module> {
    let (i, line) = located_keyword("module")(i)?;
    let (i, name) = ident(i)?;
    let (i, _) = sym("{")(i)?;
    let (i, items) = many0(item)(i)?;
    let (i, _) = sym("}")(i)?;
    let (i, _) = opt(sym(";"))(i)?;
    Ok((i, Module { name, items, line }))
}

fn struct_def(i: Span) -> IResult<Span, StructDef> {
    let (i, anns) = many0(annotation)(i)?;
    let (i, line) = located_keyword("struct")(i)?;
    let (i, name) = ident(i)?;
    let (i, _) = sym("{")(i)?;
    let (i, members) = many0(member)(i)?;
    let (i, _) = sym("}")(i)?;
    let (i, _) = opt(sym(";"))(i)?;
    Ok((
        i,
        StructDef {
            name,
            anns,
            members,
            line,
        },
    ))
}

fn member(i: Span) -> IResult<Span, Member> {
    let (i, anns) = many0(annotation)(i)?;
    let (i, _) = ws0(i)?;
    let (i, pos) = position(i)?;
    let (i, ty) = type_ref(i)?;
    let (i, name) = ident(i)?;
    let (i, _) = sym(";")(i)?;
    Ok((
        i,
        Member {
            name,
            ty,
            anns,
            line: pos.location_line(),
        },
    ))
}

fn enum_def(i: Span) -> IResult<Span, EnumDef> {
    let (i, anns) = many0(annotation)(i)?;
    let (i, line) = located_keyword("enum")(i)?;
    let (i, name) = ident(i)?;
    let (i, _) = sym("{")(i)?;
    let (i, variants) = separated_list0(sym(","), ident)(i)?;
    let (i, _) = opt(sym(","))(i)?;
    let (i, _) = sym("}")(i)?;
    let (i, _) = opt(sym(";"))(i)?;
    Ok((
        i,
        EnumDef {
            name,
            anns,
            variants,
            line,
        },
    ))
}

fn typedef_def(i: Span) -> IResult<Span, TypedefDef> {
    let (i, line) = located_keyword("typedef")(i)?;
    // Four shapes: sequence<..>, string<..>, char NAME[..], or `TYPE NAME`.
    if let Ok((i, td)) = typedef_sequence(i, line) {
        return Ok((i, td));
    }
    if let Ok((i, td)) = typedef_string(i, line) {
        return Ok((i, td));
    }
    if let Ok((i, td)) = typedef_char_array(i, line) {
        return Ok((i, td));
    }
    typedef_alias(i, line)
}

fn typedef_sequence(i: Span, line: u32) -> IResult<Span, TypedefDef> {
    let (i, ty) = sequence_type(i)?;
    let (i, name) = ident(i)?;
    let (i, _) = sym(";")(i)?;
    Ok((i, TypedefDef { name, ty, line }))
}

fn typedef_string(i: Span, line: u32) -> IResult<Span, TypedefDef> {
    let (i, ty) = bounded_string_type(i)?;
    let (i, name) = ident(i)?;
    let (i, _) = sym(";")(i)?;
    Ok((i, TypedefDef { name, ty, line }))
}

fn typedef_char_array(i: Span, line: u32) -> IResult<Span, TypedefDef> {
    let (i, _) = sym("char")(i)?;
    let (i, name) = ident(i)?;
    let (i, _) = sym("[")(i)?;
    let (i, b) = bound(i)?;
    let (i, _) = sym("]")(i)?;
    let (i, _) = sym(";")(i)?;
    Ok((
        i,
        TypedefDef {
            name,
            ty: TypeRef::CharArray(b),
            line,
        },
    ))
}

fn typedef_alias(i: Span, line: u32) -> IResult<Span, TypedefDef> {
    let (i, ty) = named_type(i)?;
    let (i, name) = ident(i)?;
    let (i, _) = sym(";")(i)?;
    Ok((i, TypedefDef { name, ty, line }))
}

fn const_def(i: Span) -> IResult<Span, ConstDef> {
    let (i, line) = located_keyword("const")(i)?;
    let (i, _type) = qualified_ident(i)?; // const element type — parsed and discarded
    let (i, name) = ident(i)?;
    let (i, _) = sym("=")(i)?;
    let (i, value) = expr(i)?;
    let (i, _) = sym(";")(i)?;
    Ok((i, ConstDef { name, value, line }))
}

fn expr(i: Span) -> IResult<Span, Expr> {
    alt((
        map(string_lit, Expr::Str),
        map(float_lit, Expr::Float),
        map(integer, Expr::Int),
        map(qualified_ident, Expr::Ident),
    ))(i)
}
