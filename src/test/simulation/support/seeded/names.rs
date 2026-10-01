use super::rng::SeededRng;
use super::SeededNames;

impl SeededNames {
    pub(super) fn new(rng: &mut SeededRng) -> Self {
        let mut allocator = SeededNameAllocator::new();
        let audit_root = allocator.constant(rng);
        let payment_root = allocator.constant(rng);
        let billing_root = allocator.constant(rng);
        let catalog_root = allocator.constant(rng);
        let reporting_root = allocator.constant(rng);

        let trackable = fqn(&[&audit_root, &allocator.constant(rng)]);
        let trackable_hook_module = fqn(&[&trackable, &allocator.constant(rng)]);
        let trackable_hook_include_module = fqn(&[&trackable, &allocator.constant(rng)]);
        let trackable_hook_class_eval_module = fqn(&[&trackable, &allocator.constant(rng)]);
        let trackable_concern_class_methods_module = fqn(&[&trackable, &allocator.constant(rng)]);
        let visibility_hidden_mixin = fqn(&[&audit_root, &allocator.constant(rng)]);
        let visibility_hidden_user = fqn(&[&audit_root, &allocator.constant(rng)]);
        let visibility_public_mixin = fqn(&[&audit_root, &allocator.constant(rng)]);
        let visibility_public_user = fqn(&[&audit_root, &allocator.constant(rng)]);
        let gateway = fqn(&[&payment_root, &allocator.constant(rng)]);
        let fallback_gateway = fqn(&[&payment_root, &allocator.constant(rng)]);
        let base_invoice = fqn(&[&billing_root, &allocator.constant(rng)]);
        let account = fqn(&[&billing_root, &allocator.constant(rng)]);
        let invoice = fqn(&[&billing_root, &allocator.constant(rng)]);
        let sku = fqn(&[&catalog_root, &allocator.constant(rng)]);
        let item = fqn(&[&catalog_root, &allocator.constant(rng)]);
        let summary = fqn(&[&reporting_root, &allocator.constant(rng)]);

        let level_constant = allocator.constant(rng).to_ascii_uppercase();
        let provider_constant = allocator.constant(rng).to_ascii_uppercase();
        let base_status_constant = allocator.constant(rng).to_ascii_uppercase();
        let currency_constant = allocator.constant(rng).to_ascii_uppercase();
        let prefix_constant = allocator.constant(rng).to_ascii_uppercase();

        let audit_method = allocator.method(rng);
        let hook_status_method = allocator.method(rng);
        let hook_render_method = allocator.method(rng);
        let hook_api_method = allocator.method(rng);
        let concern_lookup_method = allocator.method(rng);
        let visibility_hidden_method = allocator.method(rng);
        let visibility_public_method = allocator.method(rng);
        let record_method = allocator.method(rng);
        let tagged_method = allocator.method(rng);
        let capture_method = allocator.method(rng);
        let refund_method = allocator.method(rng);
        let const_get_method = allocator.method(rng);
        let default_method = allocator.method(rng);
        let void_method = allocator.method(rng);
        let private_method = allocator.method(rng);
        let private_probe_method = allocator.method(rng);
        let provider_method = allocator.method(rng);
        let queue_method = allocator.method(rng);
        let normalize_method = allocator.method(rng);
        let super_method = allocator.method(rng);
        let base_status_method = allocator.method(rng);
        let gateway_method = allocator.method(rng);
        let backup_gateway_method = allocator.method(rng);
        let account_reopen_method = allocator.method(rng);
        let charge_method = allocator.method(rng);
        let delegated_capture_method = allocator.method(rng);
        let block_scoped_method = allocator.method(rng);
        let chain_charge_method = allocator.method(rng);
        let constructor_charge_method = allocator.method(rng);
        let format_method = allocator.method(rng);
        let audit_sku_method = allocator.method(rng);
        let sku_method = allocator.method(rng);
        let publish_method = allocator.method(rng);
        let render_method = allocator.method(rng);
        let capture_total_method = allocator.method(rng);
        let dynamic_record = fqn(&[&reporting_root, &allocator.constant(rng)]);
        let dynamic_virtual_method = allocator.method(rng);

        Self {
            invoice_file: file_for_fqn(&invoice),
            account_reopen_file: reopen_file_for_fqn(&account),
            level_constant_fqn: format!("{trackable}::{level_constant}"),
            provider_constant_fqn: format!("{gateway}::{provider_constant}"),
            base_status_constant_fqn: format!("{base_invoice}::{base_status_constant}"),
            currency_constant_fqn: format!("{invoice}::{currency_constant}"),
            prefix_constant_fqn: format!("{sku}::{prefix_constant}"),
            trackable,
            trackable_hook_module,
            trackable_hook_include_module,
            trackable_hook_class_eval_module,
            trackable_concern_class_methods_module,
            visibility_hidden_mixin,
            visibility_hidden_user,
            visibility_public_mixin,
            visibility_public_user,
            gateway,
            fallback_gateway,
            base_invoice,
            account,
            invoice,
            sku,
            item,
            dynamic_record,
            summary,
            level_constant,
            provider_constant,
            base_status_constant,
            currency_constant,
            prefix_constant,
            audit_method,
            hook_status_method,
            hook_render_method,
            hook_api_method,
            concern_lookup_method,
            visibility_hidden_method,
            visibility_public_method,
            record_method,
            tagged_method,
            capture_method,
            refund_method,
            const_get_method,
            default_method,
            void_method,
            private_method,
            private_probe_method,
            provider_method,
            queue_method,
            normalize_method,
            super_method,
            base_status_method,
            gateway_method,
            backup_gateway_method,
            account_reopen_method,
            charge_method,
            delegated_capture_method,
            block_scoped_method,
            chain_charge_method,
            constructor_charge_method,
            format_method,
            audit_sku_method,
            sku_method,
            publish_method,
            render_method,
            capture_total_method,
            dynamic_virtual_method,
            gateway_local: allocator.local(rng),
            gateway_ivar: allocator.local(rng),
            item_local: allocator.local(rng),
            account_local: allocator.local(rng),
            block_item_local: allocator.local(rng),
            yield_item_local: allocator.local(rng),
            visibility_hidden_local: allocator.local(rng),
            visibility_public_local: allocator.local(rng),
            account_reopen_local: allocator.local(rng),
        }
    }

    pub(super) fn capture_target(&self) -> String {
        format!("{}#{}", self.gateway, self.capture_method)
    }

    pub(super) fn refund_target(&self) -> String {
        format!("{}#{}", self.gateway, self.refund_method)
    }

    pub(super) fn const_get_target(&self) -> String {
        format!("{}#{}", self.gateway, self.const_get_method)
    }

    pub(super) fn private_target(&self) -> String {
        format!("{}#{}", self.gateway, self.private_method)
    }

    pub(super) fn default_target(&self) -> String {
        format!("{}.{}", self.gateway, self.default_method)
    }

    pub(super) fn gateway_target(&self) -> String {
        format!("{}#{}", self.invoice, self.gateway_method)
    }

    pub(super) fn account_reopen_target(&self) -> String {
        format!("{}#{}", self.account, self.account_reopen_method)
    }

    pub(super) fn audit_target(&self) -> String {
        format!("{}#{}", self.trackable, self.audit_method)
    }

    pub(super) fn hook_status_target(&self) -> String {
        format!("{}#{}", self.trackable_hook_module, self.hook_status_method)
    }

    pub(super) fn hook_render_target(&self) -> String {
        format!(
            "{}#{}",
            self.trackable_hook_include_module, self.hook_render_method
        )
    }

    pub(super) fn hook_api_target(&self) -> String {
        format!(
            "{}#{}",
            self.trackable_hook_class_eval_module, self.hook_api_method
        )
    }

    pub(super) fn concern_lookup_target(&self) -> String {
        format!(
            "{}#{}",
            self.trackable_concern_class_methods_module, self.concern_lookup_method
        )
    }

    pub(super) fn visibility_hidden_target(&self) -> String {
        format!(
            "{}#{}",
            self.visibility_hidden_mixin, self.visibility_hidden_method
        )
    }

    pub(super) fn visibility_public_target(&self) -> String {
        format!(
            "{}#{}",
            self.visibility_public_mixin, self.visibility_public_method
        )
    }

    pub(super) fn record_target(&self) -> String {
        format!("{}#{}", self.trackable, self.record_method)
    }

    pub(super) fn tagged_target(&self) -> String {
        format!("{}#{}", self.trackable, self.tagged_method)
    }

    pub(super) fn normalize_target(&self) -> String {
        format!("{}#{}", self.base_invoice, self.normalize_method)
    }

    pub(super) fn super_target(&self) -> String {
        format!("{}#{}", self.base_invoice, self.super_method)
    }

    pub(super) fn format_target(&self) -> String {
        format!("{}#{}", self.sku, self.format_method)
    }

    pub(super) fn charge_target(&self) -> String {
        format!("{}#{}", self.invoice, self.charge_method)
    }

    pub(super) fn delegated_capture_target(&self) -> String {
        format!("{}#{}", self.invoice, self.delegated_capture_method)
    }

    pub(super) fn publish_target(&self) -> String {
        format!("{}#{}", self.item, self.publish_method)
    }

    pub(super) fn dynamic_virtual_target(&self) -> String {
        format!("{}#{}", self.dynamic_record, self.dynamic_virtual_method)
    }
}

pub(super) struct SeededNameAllocator {
    used_constants: Vec<String>,
    used_methods: Vec<String>,
    used_locals: Vec<String>,
}

impl SeededNameAllocator {
    pub(super) fn new() -> Self {
        Self {
            used_constants: Vec::new(),
            used_methods: Vec::new(),
            used_locals: Vec::new(),
        }
    }

    pub(super) fn constant(&mut self, rng: &mut SeededRng) -> String {
        loop {
            let value = format!("N{:016x}", rng.next_u64());
            if !self.used_constants.contains(&value) {
                self.used_constants.push(value.clone());
                return value;
            }
        }
    }

    pub(super) fn method(&mut self, rng: &mut SeededRng) -> String {
        loop {
            let value = format!("m_{:016x}", rng.next_u64());
            if !RUBY_RESERVED.contains(&value.as_str()) && !self.used_methods.contains(&value) {
                self.used_methods.push(value.clone());
                return value;
            }
        }
    }

    pub(super) fn local(&mut self, rng: &mut SeededRng) -> String {
        loop {
            let value = format!("v_{:016x}", rng.next_u64());
            if !RUBY_RESERVED.contains(&value.as_str()) && !self.used_locals.contains(&value) {
                self.used_locals.push(value.clone());
                return value;
            }
        }
    }
}

pub(super) const CONSTANT_WORDS: &[&str] = &[
    "atlas", "bravo", "cedar", "delta", "ember", "fable", "garnet", "harbor", "ion", "juno",
    "kava", "lumen", "mango", "nero", "onyx", "pavo", "quartz", "riven", "sable", "tavo",
];

const RUBY_RESERVED: &[&str] = &[
    "alias", "and", "begin", "break", "case", "class", "def", "defined", "do", "else", "elsif",
    "end", "ensure", "false", "for", "if", "in", "module", "next", "nil", "not", "or", "redo",
    "rescue", "retry", "return", "self", "super", "then", "true", "undef", "unless", "until",
    "when", "while", "yield",
];

pub(super) fn title_word(input: &str) -> String {
    let mut chars = input.chars();
    let first = chars
        .next()
        .expect("INVARIANT VIOLATED: empty seed word. This is a bug because name word lists must contain non-empty words. Fix: inspect CONSTANT_WORDS.");
    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
}

pub(super) fn fqn(parts: &[&str]) -> String {
    parts.join("::")
}

fn file_for_fqn(fqn: &str) -> String {
    let path = fqn
        .split("::")
        .map(underscore)
        .collect::<Vec<_>>()
        .join("/");
    format!("{path}.rb")
}

fn reopen_file_for_fqn(fqn: &str) -> String {
    let base = file_for_fqn(fqn);
    let Some(stem) = base.strip_suffix(".rb") else {
        panic!(
            "INVARIANT VIOLATED: generated Ruby file `{}` does not end in .rb. This is a bug because reopened namespace files derive from Ruby paths. Fix: inspect file_for_fqn.",
            base
        )
    };
    format!("{stem}_reopen.rb")
}

fn underscore(input: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if ch.is_uppercase() && idx > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}
