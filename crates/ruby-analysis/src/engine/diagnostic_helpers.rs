use crate::core::{MethodCallSignatureCandidate, MethodParamFact, MethodParamKind};

pub(super) const EXCEPTION_WHITELIST: &[&str] = &[
    "Exception",
    "StandardError",
    "RuntimeError",
    "ArgumentError",
    "TypeError",
    "NameError",
    "NoMethodError",
    "IOError",
    "RangeError",
    "NotImplementedError",
    "ZeroDivisionError",
    "IndexError",
    "KeyError",
    "StopIteration",
    "SystemExit",
    "Interrupt",
    "ScriptError",
    "SyntaxError",
    "LoadError",
    "LocalJumpError",
    "FrozenError",
    "EncodingError",
    "RegexpError",
    "SystemCallError",
    "ThreadError",
    "FiberError",
    "SecurityError",
    "SignalException",
];

pub(super) const NON_EXCEPTION_TYPES: &[&str] = &[
    "Integer",
    "Float",
    "Rational",
    "Complex",
    "Numeric",
    "Array",
    "Hash",
    "Symbol",
    "Regexp",
    "Range",
    "Proc",
    "Method",
    "UnboundMethod",
    "IO",
    "File",
    "Dir",
    "Time",
    "Struct",
    "Encoding",
    "Fiber",
    "Thread",
    "Mutex",
    "Queue",
    "TrueClass",
    "FalseClass",
    "NilClass",
    "Binding",
    "BasicObject",
    "Object",
];

pub(super) fn suggestion_threshold(name_len: usize) -> usize {
    match name_len {
        0..=2 => 0,
        3..=8 => 2,
        _ => 3,
    }
}

pub(super) struct MethodArity {
    pub(super) required: usize,
    pub(super) optional: usize,
    pub(super) has_rest: bool,
    pub(super) required_keywords: Vec<String>,
    pub(super) optional_keywords: Vec<String>,
    pub(super) has_kwrest: bool,
}

impl MethodArity {
    pub(super) fn from_params(params: &[MethodParamFact]) -> Self {
        let mut arity = Self {
            required: 0,
            optional: 0,
            has_rest: false,
            required_keywords: Vec::new(),
            optional_keywords: Vec::new(),
            has_kwrest: false,
        };
        for param in params {
            match param.kind {
                MethodParamKind::Required => arity.required += 1,
                MethodParamKind::Optional => arity.optional += 1,
                MethodParamKind::Rest => arity.has_rest = true,
                MethodParamKind::RequiredKeyword => {
                    arity.required_keywords.push(param.name.clone())
                }
                MethodParamKind::OptionalKeyword => {
                    arity.optional_keywords.push(param.name.clone())
                }
                MethodParamKind::KeywordRest => arity.has_kwrest = true,
                MethodParamKind::Block => {}
            }
        }
        arity
    }
}

pub(super) fn arity_mismatch(
    signature: &MethodCallSignatureCandidate,
    arity: &MethodArity,
) -> Option<(usize, Option<usize>, usize)> {
    let min = arity.required;
    let max = if arity.has_rest {
        None
    } else {
        Some(arity.required + arity.optional)
    };

    if signature.has_positional_splat {
        let too_many = max
            .map(|max| signature.positional_count > max)
            .unwrap_or(false);
        if too_many {
            return Some((min, max, signature.positional_count));
        }
        return None;
    }

    let too_few = signature.positional_count < min;
    let too_many = max
        .map(|max| signature.positional_count > max)
        .unwrap_or(false);
    if too_few || too_many {
        Some((min, max, signature.positional_count))
    } else {
        None
    }
}

pub(super) fn closest_keyword(target: &str, declared: &[String]) -> Option<String> {
    let threshold = suggestion_threshold(target.len());
    if threshold == 0 {
        return None;
    }
    let mut best: Option<(String, usize)> = None;
    for candidate in declared {
        let dist = levenshtein(candidate, target);
        if dist > threshold {
            continue;
        }
        match &best {
            Some((_, current_dist)) if *current_dist <= dist => {}
            Some(_) | None => best = Some((candidate.clone(), dist)),
        }
    }
    best.map(|(name, _)| name)
}

pub(super) fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let b_chars = b.chars().collect::<Vec<_>>();
    let mut previous = (0..=b_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; b_chars.len() + 1];

    for (i, ca) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[b_chars.len()]
}
