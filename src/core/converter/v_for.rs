use super::{
    find_dir, BaseConvertInfo, BaseConverter, BaseIR, CompilationError, ConvertInfo, CoreConverter,
    Directive, Element, ForNodeIR, ForParseResult, IRNode, JsExpr as Js,
};
use crate::core::error::CompilationErrorKind as ErrorKind;
use crate::core::tokenizer::AttributeValue;
use crate::core::util::VStr;

/// Pre converts v-if or v-for like structural dir
/// The last argument is a continuation closure for base conversion.
// continuation is from continuation passing style.
// TODO: benchmark this monster function.
pub fn pre_convert_for<'a, T, C, K>(c: &C, mut e: Element<'a>, base_convert: K) -> IRNode<T>
where
    T: ConvertInfo,
    C: CoreConverter<'a, T> + ?Sized,
    K: FnOnce(Element<'a>) -> IRNode<T>,
{
    // convert v-for, v-if is converted elsewhere
    if let Some(dir) = find_dir(&mut e, "for") {
        let b = dir.take();
        debug_assert!(find_dir(&mut e, "for").is_none());
        let n = base_convert(e);
        c.convert_for(b, n)
    } else {
        base_convert(e)
    }
}

pub fn convert_for<'a>(bc: &BaseConverter, d: Directive<'a>, n: BaseIR<'a>) -> BaseIR<'a> {
    // on empty v-for expr error
    if let Some(error) = d.check_empty_expr(ErrorKind::VForNoExpression) {
        bc.emit_error(error);
        return n;
    }
    check_template_v_for_key();
    let expr = d.expression.expect("v-for must have expression");
    let (source, parse_result) = match parse_for_expr(expr.content) {
        Some(parsed) => parsed,
        None => {
            let error = CompilationError::new(ErrorKind::VForMalformedExpression)
                .with_location(expr.location.clone());
            bc.emit_error(error);
            return n;
        }
    };
    IRNode::For(ForNodeIR {
        source,
        parse_result,
        child: Box::new(n),
    })
}

type ParsedFor<'a> = (Js<'a>, ForParseResult<BaseConvertInfo<'a>>);

fn parse_for_expr<'a>(expr: VStr<'a>) -> Option<ParsedFor<'a>> {
    // split source and binding
    let (lhs, rhs) = expr
        .raw
        .split_once(" in ")
        .or_else(|| expr.raw.split_once(" of "))
        .map(|(l, r)| (l.trim(), r.trim()))?;
    // split iterator by ,
    let (val, key, idx) = split_v_for_iter(lhs);
    Some((
        simple_var(rhs),
        ForParseResult {
            value: simple_var(val),
            key: key.map(simple_var),
            index: idx.map(simple_var),
        },
    ))
}
fn simple_var(v: &str) -> Js {
    Js::Simple(VStr::raw(v))
}

const DESTRUCTING: &'static [char] = &['}', ']'];
fn split_v_for_iter(lhs: &str) -> (&str, Option<&str>, Option<&str>) {
    let mut ret = (lhs, None, None);
    let mut split: Vec<_> = lhs.rsplitn(3, ',').map(str::trim).collect();
    split.reverse();
    if split.iter().skip(1).any(|s| s.contains(DESTRUCTING)) {
        return ret;
    }
    if split.len() == 2 {
        ret.0 = split[0];
        ret.1 = Some(split[1]);
    } else if split.len() == 3 {
        ret.0 = split[0];
        ret.1 = Some(split[1]);
        ret.2 = Some(split[2]);
    }
    ret
}

// check <template v-for> key placement
fn check_template_v_for_key() {}

#[cfg(test)]
mod test {
    use super::*;
    fn to_str(e: Js) -> &str {
        if let Js::Simple(v) = e {
            v.raw
        } else {
            panic!("invalid js expression");
        }
    }
    fn check_equal(src: &str, expect: (&str, &str, Option<&str>, Option<&str>)) {
        let (src, ret) = parse_for_expr(VStr::raw(src)).expect("should parse");
        assert_eq!(to_str(src), expect.0);
        assert_eq!(to_str(ret.value), expect.1);
        assert_eq!(ret.key.map(to_str), expect.2);
        assert_eq!(ret.index.map(to_str), expect.3);
    }
    #[test]
    fn test_parse_for_expr() {
        for (src, expect) in vec![
            ("a in [123]", ("[123]", "a", None, None)),
            ("   in [123]", ("[123]", "", None, None)),
            ("   a      in     [123]    ", ("[123]", "a", None, None)),
            ("a, b, c   in p ", ("p", "a", "b".into(), "c".into())),
            ("{a, b, c} in p ", ("p", "{a, b, c}", None, None)),
            ("{a, b}, c in p ", ("p", "{a, b}", "c".into(), None)),
            ("[a,] , b in p ", ("p", "[a,]", "b".into(), None)),
        ] {
            check_equal(src, expect);
        }
    }

    #[test]
    fn test_parse_invalid_for() {
        for src in vec![""] {
            assert!(parse_for_expr(VStr::raw(src)).is_none());
        }
    }
}
