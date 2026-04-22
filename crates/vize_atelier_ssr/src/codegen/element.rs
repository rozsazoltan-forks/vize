//! Element, component, and slot processing for SSR code generation.

use vize_atelier_core::ast::{ElementNode, ElementType, RuntimeHelper};
use vize_carton::{String, ToCompactString};

use super::{helpers::escape_html_attr, SsrCodegenContext};
use vize_carton::cstr;

impl<'a> SsrCodegenContext<'a> {
    /// Process an element node
    pub(crate) fn process_element(&mut self, el: &ElementNode, disable_nested_fragments: bool) {
        match el.tag_type {
            ElementType::Element => {
                self.process_plain_element(el);
            }
            ElementType::Component => {
                self.process_component(el, disable_nested_fragments);
            }
            ElementType::Slot => {
                self.process_slot_outlet(el);
            }
            ElementType::Template => {
                // Process template children directly
                self.process_children(&el.children, false, disable_nested_fragments, false);
            }
        }
    }

    /// Process a plain HTML element
    fn process_plain_element(&mut self, el: &ElementNode) {
        let tag = &el.tag;

        // Start tag
        self.push_string_part_static("<");
        self.push_string_part_static(tag);

        // Process attributes
        self.process_element_attrs(el);

        // Scope ID
        if let Some(scope_id) = &self.options.scope_id {
            self.push_string_part_static(" ");
            self.push_string_part_static(scope_id);
        }

        // Check if void element
        if vize_carton::is_void_tag(tag) {
            self.push_string_part_static(">");
            return;
        }

        self.push_string_part_static(">");

        // Process children
        self.process_children(&el.children, false, false, false);

        // End tag
        self.push_string_part_static("</");
        self.push_string_part_static(tag);
        self.push_string_part_static(">");
    }

    /// Process element attributes
    fn process_element_attrs(&mut self, el: &ElementNode) {
        use vize_atelier_core::ast::PropNode;

        for prop in &el.props {
            match prop {
                PropNode::Attribute(attr) => {
                    self.push_string_part_static(" ");
                    self.push_string_part_static(&attr.name);
                    if let Some(value) = &attr.value {
                        self.push_string_part_static("=\"");
                        // Escape HTML attribute value
                        self.push_string_part_static(&escape_html_attr(&value.content));
                        self.push_string_part_static("\"");
                    }
                }
                PropNode::Directive(dir) => {
                    self.process_directive_on_element(el, dir);
                }
            }
        }
    }

    /// Process a directive on an element
    fn process_directive_on_element(
        &mut self,
        el: &ElementNode,
        dir: &vize_atelier_core::ast::DirectiveNode,
    ) {
        match dir.name.as_str() {
            "bind" => {
                self.process_v_bind_on_element(el, dir);
            }
            "on" => {
                // Event handlers are ignored in SSR
            }
            "model" => {
                self.process_v_model_on_element(el, dir);
            }
            "show" => {
                self.process_v_show_on_element(el, dir);
            }
            "html" => {
                // v-html is processed when generating children
            }
            "text" => {
                // v-text is processed when generating children
            }
            _ => {
                // Custom directives: use ssrGetDirectiveProps
                self.process_custom_directive(el, dir);
            }
        }
    }

    /// Process v-bind directive
    fn process_v_bind_on_element(
        &mut self,
        _el: &ElementNode,
        dir: &vize_atelier_core::ast::DirectiveNode,
    ) {
        use vize_atelier_core::ast::ExpressionNode;

        // Get the argument (attribute name)
        let arg_name = match &dir.arg {
            Some(ExpressionNode::Simple(simple)) if simple.is_static => {
                Some(simple.content.clone())
            }
            _ => None,
        };

        // Get the expression
        let exp = match &dir.exp {
            Some(ExpressionNode::Simple(simple)) => simple.content.as_str(),
            Some(ExpressionNode::Compound(_)) => {
                // For compound expressions, we'd need to flatten - for now use placeholder
                "_ctx.value"
            }
            None => return,
        };

        match arg_name.as_deref() {
            Some("class") => {
                self.use_ssr_helper(RuntimeHelper::SsrRenderClass);
                self.push_string_part_dynamic(&cstr!("_ssrRenderClass({exp})"));
            }
            Some("style") => {
                self.use_ssr_helper(RuntimeHelper::SsrRenderStyle);
                self.push_string_part_static(" style=\"");
                self.push_string_part_dynamic(&cstr!("_ssrRenderStyle({exp})"));
                self.push_string_part_static("\"");
            }
            Some(name) => {
                self.use_ssr_helper(RuntimeHelper::SsrRenderAttr);
                self.push_string_part_dynamic(&cstr!("_ssrRenderAttr(\"{name}\", {exp})"));
            }
            None => {
                // v-bind without argument - spread attributes
                self.use_ssr_helper(RuntimeHelper::SsrRenderAttrs);
                self.push_string_part_dynamic(&cstr!("_ssrRenderAttrs({exp})"));
            }
        }
    }

    /// Process v-model directive
    fn process_v_model_on_element(
        &mut self,
        el: &ElementNode,
        dir: &vize_atelier_core::ast::DirectiveNode,
    ) {
        use vize_atelier_core::ast::ExpressionNode;

        let exp = match &dir.exp {
            Some(ExpressionNode::Simple(simple)) => simple.content.as_str(),
            _ => return,
        };

        let tag = el.tag.as_str();

        match tag {
            "input" => {
                // Check input type from attributes
                let input_type = self.get_element_attr_value(el, "type");
                match input_type.as_deref() {
                    Some("checkbox") => {
                        self.use_ssr_helper(RuntimeHelper::SsrIncludeBooleanAttr);
                        self.use_ssr_helper(RuntimeHelper::SsrLooseContain);
                        self.push_string_part_dynamic(&cstr!(
                            "(_ssrIncludeBooleanAttr(Array.isArray({exp}) ? _ssrLooseContain({exp}, null) : {exp})) ? \" checked\" : \"\""
                        ));
                    }
                    Some("radio") => {
                        self.use_ssr_helper(RuntimeHelper::SsrIncludeBooleanAttr);
                        self.use_ssr_helper(RuntimeHelper::SsrLooseEqual);
                        let value = self.get_element_attr_value(el, "value");
                        let value_exp = value.as_deref().unwrap_or("null");
                        self.push_string_part_dynamic(&cstr!(
                            "(_ssrIncludeBooleanAttr(_ssrLooseEqual({exp}, {value_exp}))) ? \" checked\" : \"\""
                        ));
                    }
                    _ => {
                        // text input
                        self.use_ssr_helper(RuntimeHelper::SsrRenderAttr);
                        self.push_string_part_dynamic(&cstr!("_ssrRenderAttr(\"value\", {exp})"));
                    }
                }
            }
            "textarea" => {
                // textarea value is set as content
                self.use_ssr_helper(RuntimeHelper::SsrInterpolate);
                // Note: will be handled when processing children
            }
            "select" => {
                // select value is handled on child options
            }
            _ => {}
        }
    }

    /// Process v-show directive
    fn process_v_show_on_element(
        &mut self,
        _el: &ElementNode,
        dir: &vize_atelier_core::ast::DirectiveNode,
    ) {
        use vize_atelier_core::ast::ExpressionNode;

        let exp = match &dir.exp {
            Some(ExpressionNode::Simple(simple)) => simple.content.as_str(),
            _ => return,
        };

        // v-show="expr" => style="display: none" if !expr
        self.push_string_part_dynamic(&cstr!(
            "(({exp}) ? \"\" : \" style=\\\"display: none;\\\"\")"
        ));
    }

    /// Process a custom directive
    fn process_custom_directive(
        &mut self,
        _el: &ElementNode,
        dir: &vize_atelier_core::ast::DirectiveNode,
    ) {
        self.use_ssr_helper(RuntimeHelper::SsrGetDirectiveProps);
        // Custom directives use ssrGetDirectiveProps to merge props
        self.push_string_part_dynamic(&cstr!(
            "_ssrRenderAttrs(_ssrGetDirectiveProps(_ctx, _directives, \"{}\"))",
            dir.name
        ));
    }

    /// Get an attribute value from an element
    pub(crate) fn get_element_attr_value(&self, el: &ElementNode, name: &str) -> Option<String> {
        use vize_atelier_core::ast::PropNode;

        for prop in &el.props {
            if let PropNode::Attribute(attr) = prop {
                if attr.name == name {
                    return attr.value.as_ref().map(|v| v.content.to_compact_string());
                }
            }
        }
        None
    }

    /// Process a component
    fn process_component(&mut self, el: &ElementNode, _disable_nested_fragments: bool) {
        self.flush_push();
        self.use_ssr_helper(RuntimeHelper::SsrRenderComponent);

        let tag = &el.tag;
        let setup_binding = self.resolve_component_binding_name(tag);
        let uses_setup_binding = setup_binding.is_some();

        self.push_indent();
        self.push("_push(_ssrRenderComponent(");
        if let Some(binding_name) = setup_binding.as_deref() {
            if !self.options.inline {
                self.push("$setup.");
            }
            self.push(binding_name);
        } else {
            self.use_core_helper(RuntimeHelper::ResolveComponent);
            self.push("_resolveComponent(\"");
            self.push(tag);
            self.push("\")");
        }
        if uses_setup_binding {
            self.push(", null, ");
        } else {
            self.push(", _attrs, ");
        }

        // Process slots
        if el.children.is_empty() {
            self.push("null");
        } else {
            self.use_core_helper(RuntimeHelper::WithCtx);
            self.push("{\n");
            self.indent_level += 1;
            self.push_indent();
            self.push("default: _withCtx((_, _push, _parent, _scopeId) => {\n");
            self.indent_level += 1;
            self.push_indent();
            self.push("if (_push) {\n");
            self.indent_level += 1;

            // Flush and start fresh for slot content
            let old_parts = std::mem::take(&mut self.current_template_parts);
            let previous_slot_scope = self.with_slot_scope_id;
            self.with_slot_scope_id = true;
            self.process_children(&el.children, false, false, false);
            self.flush_push();
            self.with_slot_scope_id = previous_slot_scope;
            self.current_template_parts = old_parts;

            self.indent_level -= 1;
            self.push_indent();
            self.push("}\n");
            self.indent_level -= 1;
            self.push_indent();
            self.push("}),\n");
            self.indent_level -= 1;
            self.push_indent();
            self.push("_: 1\n");
            self.push_indent();
            self.push("}");
        }

        self.push(", _parent");
        if self.with_slot_scope_id && self.options.scope_id.is_some() {
            self.push(", _scopeId");
        }
        self.push("))\n");
    }

    /// Process a slot outlet (<slot>)
    fn process_slot_outlet(&mut self, el: &ElementNode) {
        self.flush_push();
        self.use_ssr_helper(RuntimeHelper::SsrRenderSlot);

        self.push_indent();
        self.push("_ssrRenderSlot(_ctx.$slots, ");

        // Get slot name
        let slot_name = self.get_slot_name(el);
        self.push("\"");
        self.push(&slot_name);
        self.push("\", ");

        // Slot props
        self.push("{}, ");

        // Fallback content
        if el.children.is_empty() {
            self.push("null");
        } else {
            self.push("() => {\n");
            self.indent_level += 1;

            let old_parts = std::mem::take(&mut self.current_template_parts);
            self.process_children(&el.children, false, false, false);
            self.flush_push();
            self.current_template_parts = old_parts;

            self.indent_level -= 1;
            self.push_indent();
            self.push("}");
        }

        self.push(", _push, _parent");

        // Scope ID
        if self.options.scope_id.is_some() {
            self.push(", _scopeId");
        }

        self.push(")\n");
    }

    /// Get the name of a slot
    fn get_slot_name(&self, el: &ElementNode) -> String {
        use vize_atelier_core::ast::{ExpressionNode, PropNode};

        for prop in &el.props {
            if let PropNode::Directive(dir) = prop {
                if dir.name == "bind" {
                    if let Some(ExpressionNode::Simple(arg)) = &dir.arg {
                        if arg.content == "name" {
                            if let Some(ExpressionNode::Simple(exp)) = &dir.exp {
                                return exp.content.to_compact_string();
                            }
                        }
                    }
                }
            } else if let PropNode::Attribute(attr) = prop {
                if attr.name == "name" {
                    if let Some(value) = &attr.value {
                        return value.content.to_compact_string();
                    }
                }
            }
        }
        "default".to_compact_string()
    }
}
