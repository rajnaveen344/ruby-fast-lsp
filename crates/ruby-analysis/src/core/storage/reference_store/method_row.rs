//! The stored form of one method reference candidate.
//!
//! Ordinary calls keep every field inline: ranges inside the call's own file
//! are byte spans, receiver classes and modules are interned names, and the
//! receiver label is either the receiver type itself or interned identifier
//! text. Rare payloads (keyword arguments, structural receiver types,
//! rendered labels, explicit definition ranges, and a diagnostic range that
//! differs from the reference range) live in one cold box allocated only when
//! present.

use std::mem::size_of;

use ustr::Ustr;

use super::candidate::{
    KeywordArgCandidate, MethodCallSignatureCandidate, MethodReceiverLabel,
    MethodReferenceDiagnostics,
};
use super::MethodReferenceAccess;
use crate::core::names::fqn_id::{ConstLookupId, FqnId, OptionalFqnId};
use crate::core::storage::memory_estimate::ruby_type_heap_bytes;
use crate::core::{FullyQualifiedName, NamespaceKind, RubyMethod, RubyType, TextRange};
use crate::invariant::ExpectInvariant;

/// An optional byte span in the same file as its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SameFileSpan {
    start: u32,
    end: u32,
}

impl SameFileSpan {
    const NONE: Self = Self {
        start: u32::MAX,
        end: 0,
    };

    fn new(row: TextRange, range: Option<TextRange>) -> Self {
        let Some(range) = range else {
            return Self::NONE;
        };
        invariant!(
            range.file_id == row.file_id && range.start_byte <= range.end_byte,
            what = "method reference span is not a range in the reference's file",
            why = "stored method reference spans share the row's source file",
            fix = "attach only ranges from the reference's own file to method candidates",
        );
        Self {
            start: range.start_byte,
            end: range.end_byte,
        }
    }

    fn get(self, row: TextRange) -> Option<TextRange> {
        (self != Self::NONE).then(|| TextRange::new(row.file_id, self.start, self.end))
    }
}

/// How the receiver type is stored inline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReceiverForm {
    None = 0,
    Class = 1,
    Module = 2,
    ClassReference = 3,
    ModuleReference = 4,
    Cold = 5,
}

const OWNER_SINGLETON: u32 = 1 << 0;
const ACCESS_SHIFT: u32 = 1;
const ACCESS_MASK: u32 = 0b11 << ACCESS_SHIFT;
const RECEIVER_SHIFT: u32 = 3;
const RECEIVER_MASK: u32 = 0b111 << RECEIVER_SHIFT;
const IS_SUPER: u32 = 1 << 6;
const HAS_DIAGNOSTICS: u32 = 1 << 7;
const DIAGNOSE_UNRESOLVED: u32 = 1 << 8;
const ALLOW_UNINDEXED_OWNER: u32 = 1 << 9;
const SAFE_NAVIGATION: u32 = 1 << 10;
const HAS_SIGNATURE: u32 = 1 << 11;
const HAS_POSITIONAL_SPLAT: u32 = 1 << 12;
const HAS_NONEMPTY_KEYWORD_HASH: u32 = 1 << 13;
const TRAILING_POSITIONAL_MAY_BE_OPTIONS_HASH: u32 = 1 << 14;
const HAS_KEYWORD_SPLAT: u32 = 1 << 15;
const LABEL_IS_RECEIVER_TYPE: u32 = 1 << 16;

/// Payloads that ordinary calls never carry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ColdMethodReference {
    preferred_definition_range: Option<TextRange>,
    /// Present only when the diagnostic range differs from the row range.
    diagnostic_range: Option<TextRange>,
    receiver_label: Option<Box<str>>,
    receiver_type: Option<RubyType>,
    keyword_args: Box<[KeywordArgCandidate]>,
}

impl ColdMethodReference {
    fn is_empty(&self) -> bool {
        self.preferred_definition_range.is_none()
            && self.diagnostic_range.is_none()
            && self.receiver_label.is_none()
            && self.receiver_type.is_none()
            && self.keyword_args.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMethodReferenceCandidate {
    pub range: TextRange,
    owner: ConstLookupId,
    method: RubyMethod,
    caller: OptionalFqnId,
    call_expression: SameFileSpan,
    receiver_expression: SameFileSpan,
    receiver_fqn: OptionalFqnId,
    label: Option<Ustr>,
    cold: Option<Box<ColdMethodReference>>,
    positional_count: u32,
    flags: u32,
}

/// The receiver type a call site proved, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredReceiverType<'a> {
    /// A class or module type; expand the interned name to rebuild it.
    Named(NamedReceiverForm, FqnId),
    Structural(&'a RubyType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedReceiverForm {
    Class,
    Module,
    ClassReference,
    ModuleReference,
}

impl StoredReceiverType<'_> {
    pub fn expand(self, fqn: impl FnOnce(FqnId) -> FullyQualifiedName) -> RubyType {
        match self {
            Self::Named(form, id) => {
                let fqn = fqn(id);
                match form {
                    NamedReceiverForm::Class => RubyType::Class(fqn),
                    NamedReceiverForm::Module => RubyType::Module(fqn),
                    NamedReceiverForm::ClassReference => RubyType::ClassReference(fqn),
                    NamedReceiverForm::ModuleReference => RubyType::ModuleReference(fqn),
                }
            }
            Self::Structural(ruby_type) => ruby_type.clone(),
        }
    }
}

/// How an unresolved-method message names the receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredReceiverLabel<'a> {
    /// Render the stored receiver type.
    ReceiverType,
    Text(&'a str),
}

/// Call shape of an invocation, borrowed from its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodCallSignature<'a> {
    pub positional_count: usize,
    pub has_positional_splat: bool,
    pub has_nonempty_keyword_hash: bool,
    pub trailing_positional_may_be_options_hash: bool,
    pub keyword_args: &'a [KeywordArgCandidate],
    pub has_keyword_splat: bool,
}

/// Diagnostic evidence of one method reference, borrowed from its row.
#[derive(Debug, Clone, Copy)]
pub struct MethodCallDiagnostics<'a> {
    row: &'a StoredMethodReferenceCandidate,
}

impl StoredMethodReferenceCandidate {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        range: TextRange,
        owner: ConstLookupId,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        is_super: bool,
        access: MethodReferenceAccess,
        caller: Option<FqnId>,
        call_expression_range: Option<TextRange>,
        preferred_definition_range: Option<TextRange>,
        diagnostics: Option<MethodReferenceDiagnostics>,
        mut intern: impl FnMut(FullyQualifiedName) -> FqnId,
    ) -> Self {
        let mut flags = access_bits(access) << ACCESS_SHIFT;
        if owner_kind == NamespaceKind::Singleton {
            flags |= OWNER_SINGLETON;
        }
        if is_super {
            flags |= IS_SUPER;
        }
        let mut cold = ColdMethodReference {
            preferred_definition_range,
            ..ColdMethodReference::default()
        };
        let mut row = Self {
            range,
            owner,
            method,
            caller: caller.into(),
            call_expression: SameFileSpan::new(range, call_expression_range),
            receiver_expression: SameFileSpan::NONE,
            receiver_fqn: OptionalFqnId::NONE,
            label: None,
            cold: None,
            positional_count: 0,
            flags: 0,
        };
        if let Some(diagnostics) = diagnostics {
            flags |= HAS_DIAGNOSTICS;
            if diagnostics.diagnostic_range != range {
                cold.diagnostic_range = Some(diagnostics.diagnostic_range);
            }
            row.receiver_expression =
                SameFileSpan::new(range, diagnostics.receiver_expression_range);
            let form = match diagnostics.receiver_type.map(|ruby_type| *ruby_type) {
                None => ReceiverForm::None,
                Some(RubyType::Class(fqn)) => {
                    row.receiver_fqn = Some(intern(fqn)).into();
                    ReceiverForm::Class
                }
                Some(RubyType::Module(fqn)) => {
                    row.receiver_fqn = Some(intern(fqn)).into();
                    ReceiverForm::Module
                }
                Some(RubyType::ClassReference(fqn)) => {
                    row.receiver_fqn = Some(intern(fqn)).into();
                    ReceiverForm::ClassReference
                }
                Some(RubyType::ModuleReference(fqn)) => {
                    row.receiver_fqn = Some(intern(fqn)).into();
                    ReceiverForm::ModuleReference
                }
                Some(
                    structural @ (RubyType::Literal(_)
                    | RubyType::Array(_)
                    | RubyType::Hash(_, _)
                    | RubyType::Shape(_)
                    | RubyType::Union(_)
                    | RubyType::Unknown),
                ) => {
                    cold.receiver_type = Some(structural);
                    ReceiverForm::Cold
                }
            };
            flags |= (form as u32) << RECEIVER_SHIFT;
            match diagnostics.receiver_label {
                None => {}
                Some(MethodReceiverLabel::ReceiverType) => {
                    invariant!(
                        form != ReceiverForm::None,
                        what = "receiver label names a receiver type the call does not have",
                        why = "the label is rendered from the stored receiver type",
                        fix = "emit a written or rendered label when no receiver type is proven",
                    );
                    flags |= LABEL_IS_RECEIVER_TYPE;
                }
                Some(MethodReceiverLabel::Written(text)) => row.label = Some(Ustr::from(&text)),
                Some(MethodReceiverLabel::Rendered(text)) => {
                    cold.receiver_label = Some(text.into_boxed_str())
                }
            }
            for (enabled, bit) in [
                (diagnostics.diagnose_unresolved, DIAGNOSE_UNRESOLVED),
                (diagnostics.allow_unindexed_owner, ALLOW_UNINDEXED_OWNER),
                (diagnostics.safe_navigation, SAFE_NAVIGATION),
            ] {
                if enabled {
                    flags |= bit;
                }
            }
            if let Some(signature) = diagnostics.signature {
                flags |= HAS_SIGNATURE | signature_bits(&signature);
                row.positional_count = u32::try_from(signature.positional_count).expect_invariant(
                    "call site has more positional arguments than u32::MAX",
                    "a source file cannot hold that many arguments",
                    "reject such sources before indexing",
                );
                cold.keyword_args = signature.keyword_args.into_boxed_slice();
            }
        }
        row.flags = flags;
        row.cold = (!cold.is_empty()).then(|| Box::new(cold));
        row
    }

    pub fn owner(&self) -> ConstLookupId {
        self.owner
    }

    pub fn owner_kind(&self) -> NamespaceKind {
        if self.flags & OWNER_SINGLETON != 0 {
            NamespaceKind::Singleton
        } else {
            NamespaceKind::Instance
        }
    }

    pub fn method(&self) -> RubyMethod {
        self.method
    }

    pub fn is_super(&self) -> bool {
        self.flags & IS_SUPER != 0
    }

    pub fn access(&self) -> MethodReferenceAccess {
        match (self.flags & ACCESS_MASK) >> ACCESS_SHIFT {
            0 => MethodReferenceAccess::Normal,
            1 => MethodReferenceAccess::ExplicitReceiver,
            2 => MethodReferenceAccess::VisibilityBypass,
            3 => MethodReferenceAccess::InstanceMethodReflection,
            _ => unreachable_invariant!(
                what = "method reference access bits hold a value wider than two bits",
                why = "ACCESS_MASK keeps exactly two bits",
                fix = "keep access_bits and ACCESS_MASK in sync",
            ),
        }
    }

    pub fn caller(&self) -> Option<FqnId> {
        self.caller.get()
    }

    /// The same call site naming another method, for rename collision checks.
    pub(crate) fn renamed(&self, method: RubyMethod) -> Self {
        Self {
            method,
            ..self.clone()
        }
    }

    pub fn call_expression_range(&self) -> Option<TextRange> {
        self.call_expression.get(self.range)
    }

    pub fn preferred_definition_range(&self) -> Option<TextRange> {
        self.cold.as_ref()?.preferred_definition_range
    }

    pub fn diagnostics(&self) -> Option<MethodCallDiagnostics<'_>> {
        (self.flags & HAS_DIAGNOSTICS != 0).then_some(MethodCallDiagnostics { row: self })
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.cold.as_deref().map_or(0, |cold| {
            size_of::<ColdMethodReference>()
                + cold.receiver_label.as_ref().map_or(0, |label| label.len())
                + cold.receiver_type.as_ref().map_or(0, ruby_type_heap_bytes)
                + std::mem::size_of_val::<[KeywordArgCandidate]>(&cold.keyword_args)
        })
    }

    fn flag(&self, bit: u32) -> bool {
        self.flags & bit != 0
    }

    fn receiver_form(&self) -> ReceiverForm {
        match (self.flags & RECEIVER_MASK) >> RECEIVER_SHIFT {
            0 => ReceiverForm::None,
            1 => ReceiverForm::Class,
            2 => ReceiverForm::Module,
            3 => ReceiverForm::ClassReference,
            4 => ReceiverForm::ModuleReference,
            5 => ReceiverForm::Cold,
            _ => unreachable_invariant!(
                what = "method reference receiver bits name no receiver form",
                why = "the constructor stores only ReceiverForm discriminants",
                fix = "keep ReceiverForm and the receiver bit decoding in sync",
            ),
        }
    }
}

impl<'a> MethodCallDiagnostics<'a> {
    pub fn diagnostic_range(self) -> TextRange {
        self.row
            .cold
            .as_ref()
            .and_then(|cold| cold.diagnostic_range)
            .unwrap_or(self.row.range)
    }

    pub fn receiver_expression_range(self) -> Option<TextRange> {
        self.row.receiver_expression.get(self.row.range)
    }

    pub fn receiver_type(self) -> Option<StoredReceiverType<'a>> {
        let named = |form| {
            let id = self.row.receiver_fqn.get().expect_invariant(
                "named method receiver has no interned name",
                "the constructor interns every class or module receiver",
                "store the receiver FQN id with its form",
            );
            Some(StoredReceiverType::Named(form, id))
        };
        match self.row.receiver_form() {
            ReceiverForm::None => None,
            ReceiverForm::Class => named(NamedReceiverForm::Class),
            ReceiverForm::Module => named(NamedReceiverForm::Module),
            ReceiverForm::ClassReference => named(NamedReceiverForm::ClassReference),
            ReceiverForm::ModuleReference => named(NamedReceiverForm::ModuleReference),
            ReceiverForm::Cold => {
                let ruby_type = self
                    .row
                    .cold
                    .as_ref()
                    .and_then(|cold| cold.receiver_type.as_ref())
                    .expect_invariant(
                        "structural method receiver has no cold payload",
                        "the constructor moves every structural receiver into the cold box",
                        "allocate the cold box whenever a structural receiver is stored",
                    );
                Some(StoredReceiverType::Structural(ruby_type))
            }
        }
    }

    pub fn receiver_label(self) -> Option<StoredReceiverLabel<'a>> {
        if self.row.flag(LABEL_IS_RECEIVER_TYPE) {
            return Some(StoredReceiverLabel::ReceiverType);
        }
        if let Some(label) = self.row.label {
            return Some(StoredReceiverLabel::Text(label.as_str()));
        }
        self.row
            .cold
            .as_ref()
            .and_then(|cold| cold.receiver_label.as_deref())
            .map(StoredReceiverLabel::Text)
    }

    pub fn diagnose_unresolved(self) -> bool {
        self.row.flag(DIAGNOSE_UNRESOLVED)
    }

    pub fn allow_unindexed_owner(self) -> bool {
        self.row.flag(ALLOW_UNINDEXED_OWNER)
    }

    pub fn safe_navigation(self) -> bool {
        self.row.flag(SAFE_NAVIGATION)
    }

    pub fn signature(self) -> Option<MethodCallSignature<'a>> {
        let row = self.row;
        row.flag(HAS_SIGNATURE).then(|| MethodCallSignature {
            positional_count: row.positional_count as usize,
            has_positional_splat: row.flag(HAS_POSITIONAL_SPLAT),
            has_nonempty_keyword_hash: row.flag(HAS_NONEMPTY_KEYWORD_HASH),
            trailing_positional_may_be_options_hash: row
                .flag(TRAILING_POSITIONAL_MAY_BE_OPTIONS_HASH),
            keyword_args: row.cold.as_ref().map_or(&[], |cold| &cold.keyword_args),
            has_keyword_splat: row.flag(HAS_KEYWORD_SPLAT),
        })
    }
}

fn access_bits(access: MethodReferenceAccess) -> u32 {
    match access {
        MethodReferenceAccess::Normal => 0,
        MethodReferenceAccess::ExplicitReceiver => 1,
        MethodReferenceAccess::VisibilityBypass => 2,
        MethodReferenceAccess::InstanceMethodReflection => 3,
    }
}

fn signature_bits(signature: &MethodCallSignatureCandidate) -> u32 {
    [
        (signature.has_positional_splat, HAS_POSITIONAL_SPLAT),
        (
            signature.has_nonempty_keyword_hash,
            HAS_NONEMPTY_KEYWORD_HASH,
        ),
        (
            signature.trailing_positional_may_be_options_hash,
            TRAILING_POSITIONAL_MAY_BE_OPTIONS_HASH,
        ),
        (signature.has_keyword_splat, HAS_KEYWORD_SPLAT),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .fold(0, |bits, (_, bit)| bits | bit)
}
