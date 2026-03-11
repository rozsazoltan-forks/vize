//! Main props generation logic.

use crate::ast::{ExpressionNode, PropNode, RuntimeHelper};
use vize_relief::options::BindingType;

use super::{
    super::{
        context::CodegenContext,
        helpers::{escape_js_string, is_valid_js_identifier},
    },
    directives::{generate_directive_prop_with_static, is_supported_directive},
    events::{count_event_names, generate_merged_event_handlers, get_von_event_key},
    generate_vbind_object_exp, generate_von_object_exp, has_dynamic_key, has_dynamic_vmodel,
    has_other_props, has_vbind_object, has_von_object,
};
use vize_carton::{FxHashSet, String};

/// Generate props object
pub fn generate_props(ctx: &mut CodegenContext, props: &[PropNode<'_>]) {
    // Clone scope_id to avoid borrow checker issues.
    // For component/slot elements, skip_scope_id suppresses the attribute.
    let scope_id = if ctx.skip_scope_id {
        None
    } else {
        ctx.options.scope_id.clone()
    };

    // If no props but we have scope_id, generate object with just scope_id
    if props.is_empty() {
        if let Some(ref sid) = scope_id {
            ctx.push("{ \"");
            ctx.push(sid);
            ctx.push("\": \"\" }");
        } else {
            ctx.push("null");
        }
        return;
    }

    // Check for v-bind object (v-bind="attrs") and v-on object (v-on="handlers")
    let has_vbind_obj = has_vbind_object(props);
    let has_von_obj = has_von_object(props);
    let has_other = has_other_props(props);

    // Handle cases with object spreads (v-bind="obj" or v-on="obj")
    if has_vbind_obj || has_von_obj {
        if has_other || (has_vbind_obj && has_von_obj) {
            // Multiple spreads or spread with other props: _mergeProps(...)
            ctx.use_helper(RuntimeHelper::MergeProps);
            ctx.push(ctx.helper(RuntimeHelper::MergeProps));
            ctx.push("(");

            let mut first_merge_arg = true;

            // Add v-bind object spread
            if has_vbind_obj {
                generate_vbind_object_exp(ctx, props);
                first_merge_arg = false;
            }

            // Add v-on object spread (wrapped with toHandlers)
            if has_von_obj {
                if !first_merge_arg {
                    ctx.push(", ");
                }
                generate_von_object_exp(ctx, props);
                first_merge_arg = false;
            }

            // Add other props as object (includes scope_id)
            // Inside mergeProps, skip normalizeClass/normalizeStyle - mergeProps handles it
            if has_other {
                if !first_merge_arg {
                    ctx.push(", ");
                }
                generate_props_object_inner(ctx, props, true, true);
            } else if let Some(ref sid) = scope_id {
                // No other props but we have scope_id, add it as separate object
                if !first_merge_arg {
                    ctx.push(", ");
                }
                ctx.push("{ \"");
                ctx.push(sid);
                ctx.push("\": \"\" }");
            }

            ctx.push(")");
        } else if has_vbind_obj {
            // v-bind="attrs" alone
            // If we have scope_id, we need to merge it with the bound object
            if let Some(ref sid) = scope_id {
                // _mergeProps(_normalizeProps(_guardReactiveProps(obj)), { "data-v-xxx": "" })
                ctx.use_helper(RuntimeHelper::MergeProps);
                ctx.use_helper(RuntimeHelper::NormalizeProps);
                ctx.use_helper(RuntimeHelper::GuardReactiveProps);
                ctx.push(ctx.helper(RuntimeHelper::MergeProps));
                ctx.push("(");
                ctx.push(ctx.helper(RuntimeHelper::NormalizeProps));
                ctx.push("(");
                ctx.push(ctx.helper(RuntimeHelper::GuardReactiveProps));
                ctx.push("(");
                generate_vbind_object_exp(ctx, props);
                ctx.push(")), { \"");
                ctx.push(sid);
                ctx.push("\": \"\" })");
            } else {
                // _normalizeProps(_guardReactiveProps(_ctx.attrs))
                ctx.use_helper(RuntimeHelper::NormalizeProps);
                ctx.use_helper(RuntimeHelper::GuardReactiveProps);
                ctx.push(ctx.helper(RuntimeHelper::NormalizeProps));
                ctx.push("(");
                ctx.push(ctx.helper(RuntimeHelper::GuardReactiveProps));
                ctx.push("(");
                generate_vbind_object_exp(ctx, props);
                ctx.push("))");
            }
        } else {
            // v-on="handlers" alone
            // If we have scope_id, we need to merge it with the handlers
            if let Some(ref sid) = scope_id {
                // _mergeProps(_toHandlers(handlers, true), { "data-v-xxx": "" })
                ctx.use_helper(RuntimeHelper::MergeProps);
                ctx.push(ctx.helper(RuntimeHelper::MergeProps));
                ctx.push("(");
                generate_von_object_exp(ctx, props);
                ctx.push(", { \"");
                ctx.push(sid);
                ctx.push("\": \"\" })");
            } else {
                // _toHandlers(_ctx.handlers)
                generate_von_object_exp(ctx, props);
            }
        }
        return;
    }

    // Check if we need normalizeProps wrapper
    // - dynamic v-model argument
    // - dynamic v-bind key (:[attr])
    // - dynamic v-on key (@[event])
    let has_dyn_vmodel = has_dynamic_vmodel(props);
    let has_dyn_key = has_dynamic_key(props);
    let needs_normalize = has_dyn_vmodel || has_dyn_key;
    if needs_normalize {
        ctx.use_helper(RuntimeHelper::NormalizeProps);
        ctx.push(ctx.helper(RuntimeHelper::NormalizeProps));
        ctx.push("(");
    }

    generate_props_object(ctx, props, false);

    // Close normalizeProps wrapper if needed
    if needs_normalize {
        ctx.push(")");
    }
}

/// Generate props as a regular object { key: value, ... }
fn generate_props_object(
    ctx: &mut CodegenContext,
    props: &[PropNode<'_>],
    skip_object_spreads: bool,
) {
    generate_props_object_inner(ctx, props, skip_object_spreads, false);
}

/// Generate the props object with optional class/style normalization skipping.
/// `inside_merge_props`: when true, skip normalizeClass/normalizeStyle wrappers
/// because mergeProps handles normalization internally.
fn generate_props_object_inner(
    ctx: &mut CodegenContext,
    props: &[PropNode<'_>],
    skip_object_spreads: bool,
    inside_merge_props: bool,
) {
    // When inside mergeProps, skip normalizeClass/normalizeStyle wrappers
    let prev_skip = ctx.skip_normalize;
    if inside_merge_props {
        ctx.skip_normalize = true;
    }

    // Clone scope_id to avoid borrow checker issues.
    // For component/slot elements, skip_scope_id suppresses the attribute.
    let scope_id = if ctx.skip_scope_id {
        None
    } else {
        ctx.options.scope_id.clone()
    };

    // Check for static class/style that need to be merged with dynamic
    let static_class = props.iter().find_map(|p| {
        if let PropNode::Attribute(attr) = p {
            if attr.name == "class" {
                return attr.value.as_ref().map(|v| v.content.as_str());
            }
        }
        None
    });

    let static_style = props.iter().find_map(|p| {
        if let PropNode::Attribute(attr) = p {
            if attr.name == "style" {
                return attr.value.as_ref().map(|v| v.content.as_str());
            }
        }
        None
    });

    let has_dynamic_class = props.iter().any(|p| {
        if let PropNode::Directive(dir) = p {
            if dir.name == "bind" {
                if let Some(ExpressionNode::Simple(exp)) = &dir.arg {
                    return exp.content == "class";
                }
            }
        }
        false
    });

    let has_dynamic_style = props.iter().any(|p| {
        if let PropNode::Directive(dir) = p {
            if dir.name == "bind" {
                if let Some(ExpressionNode::Simple(exp)) = &dir.arg {
                    return exp.content == "style";
                }
            }
        }
        false
    });

    // Skip static class/style if we have dynamic version (will merge them)
    let skip_static_class = static_class.is_some() && has_dynamic_class;
    let skip_static_style = static_style.is_some() && has_dynamic_style;

    // Count visible props (attributes + supported directives + scope_id if present)
    let has_scope_id = scope_id.is_some();
    let skip_is = ctx.skip_is_prop;
    let visible_count = props
        .iter()
        .filter(|p| {
            // Skip `is` prop for dynamic components
            if skip_is {
                match p {
                    PropNode::Attribute(attr) if attr.name == "is" => return false,
                    PropNode::Directive(dir)
                        if dir.name == "bind"
                            && matches!(&dir.arg, Some(ExpressionNode::Simple(exp)) if exp.content == "is") =>
                    {
                        return false
                    }
                    _ => {}
                }
            }
            match p {
                PropNode::Attribute(attr) => {
                    if skip_static_class && attr.name == "class" {
                        return false;
                    }
                    if skip_static_style && attr.name == "style" {
                        return false;
                    }
                    true
                }
                PropNode::Directive(dir) => is_supported_directive(dir),
            }
        })
        .count()
        + if has_scope_id { 1 } else { 0 };

    // Check if any prop requires a normalizer (class/style bindings) or uses helper functions (v-text)
    let has_normalizer = props.iter().any(|p| {
        if let PropNode::Directive(dir) = p {
            // v-text uses _toDisplayString, which makes the output multiline
            if dir.name == "text" {
                return true;
            }
            if dir.name == "bind" {
                if let Some(ExpressionNode::Simple(exp)) = &dir.arg {
                    return exp.content == "class" || exp.content == "style";
                }
            }
        }
        false
    });

    // Check if any v-on has inline handler (not just identifier) or has runtime modifiers
    // Also check for cached handlers which produce long expressions
    let has_inline_handler = props.iter().any(|p| {
        if let PropNode::Directive(dir) = p {
            if dir.name == "on" {
                // When cache_handlers is enabled, handlers produce long expressions
                // that need multiline formatting (except setup-const which aren't cached)
                if ctx.cache_handlers_in_current_scope() && dir.exp.is_some() {
                    let is_const = dir.exp.as_ref().is_some_and(|exp| {
                        if let ExpressionNode::Simple(simple) = exp {
                            if !simple.is_static {
                                let content = simple.content.trim();
                                if crate::transforms::is_simple_identifier(content) {
                                    if let Some(ref metadata) = ctx.options.binding_metadata {
                                        return matches!(
                                            metadata.bindings.get(content),
                                            Some(crate::options::BindingType::SetupConst)
                                        );
                                    }
                                }
                            }
                        }
                        false
                    });
                    if !is_const {
                        return true;
                    }
                }
                // Check for modifiers that will use withModifiers or withKeys (not event option modifiers)
                let has_runtime_modifier = dir.modifiers.iter().any(|m| {
                    let n = m.content.as_str();
                    // Event option modifiers (capture, once, passive) don't require multiline
                    // because they just modify the event name, not wrap the handler
                    !matches!(n, "capture" | "once" | "passive")
                });
                if has_runtime_modifier {
                    return true;
                }
                if let Some(ExpressionNode::Simple(simple)) = &dir.exp {
                    // Inline if contains operators, parens, or is not simple identifier
                    let content = simple.content.as_str();
                    return content.contains('(')
                        || content.contains('+')
                        || content.contains('-')
                        || content.contains('=')
                        || content.contains(' ');
                }
            }
        }
        false
    });

    let multiline = visible_count > 1 || has_normalizer || has_inline_handler;

    if multiline {
        ctx.push("{");
        ctx.indent();
    } else {
        ctx.push("{ ");
    }

    // Pre-scan: find duplicate v-on event names that need array merging
    let event_counts = count_event_names(props);

    let mut first = true;
    // Track which event names have already been output (for array merging)
    let mut emitted_events: FxHashSet<String> = FxHashSet::default();

    for prop in props {
        // Skip v-slot directive (handled separately in slots codegen)
        if let PropNode::Directive(dir) = prop {
            if dir.name == "slot" {
                continue;
            }
        }

        // Skip `is` prop when generating for dynamic components
        if ctx.skip_is_prop {
            match prop {
                PropNode::Attribute(attr) if attr.name == "is" => continue,
                PropNode::Directive(dir)
                    if dir.name == "bind"
                        && matches!(&dir.arg, Some(ExpressionNode::Simple(exp)) if exp.content == "is") =>
                {
                    continue
                }
                _ => {}
            }
        }

        match prop {
            PropNode::Attribute(attr) => {
                // Skip static class/style if merging with dynamic
                if skip_static_class && attr.name == "class" {
                    continue;
                }
                if skip_static_style && attr.name == "style" {
                    continue;
                }
                if !first {
                    ctx.push(",");
                }
                if multiline {
                    ctx.newline();
                } else if !first {
                    ctx.push(" ");
                }
                first = false;

                // Check if this is a ref attribute that needs ref_key generation
                let ref_binding_type = if attr.name == "ref" && ctx.options.inline {
                    attr.value.as_ref().and_then(|v| {
                        ctx.options
                            .binding_metadata
                            .as_ref()
                            .and_then(|m| m.bindings.get(v.content.as_str()).copied())
                    })
                } else {
                    None
                };
                let needs_ref_key = matches!(
                    ref_binding_type,
                    Some(
                        BindingType::SetupLet | BindingType::SetupRef | BindingType::SetupMaybeRef
                    )
                );

                if needs_ref_key {
                    // Emit ref_key + ref pair for setup-let/ref/maybe-ref bindings.
                    // Vue's runtime setRef() needs ref_key to write to instance.refs,
                    // which is essential for useTemplateRef to receive the element.
                    let ref_name = &attr.value.as_ref().unwrap().content;
                    ctx.push("ref_key: \"");
                    ctx.push(ref_name);
                    ctx.push("\", ref: ");
                    ctx.push(ref_name);
                } else {
                    // Normal attribute output
                    let needs_quotes = !is_valid_js_identifier(&attr.name);
                    if needs_quotes {
                        ctx.push("\"");
                    }
                    ctx.push(&attr.name);
                    if needs_quotes {
                        ctx.push("\"");
                    }
                    ctx.push(": ");
                    if let Some(value) = &attr.value {
                        // In inline mode, ref="refName" should reference the setup variable
                        // instead of being a string literal, if refName is a known binding
                        if ref_binding_type.is_some() {
                            ctx.push(&value.content);
                        } else {
                            ctx.push("\"");
                            ctx.push(&escape_js_string(&value.content));
                            ctx.push("\"");
                        }
                    } else {
                        ctx.push("\"\"");
                    }
                }
            }
            PropNode::Directive(dir) => {
                // Skip v-bind/v-on object spreads (handled separately by generate_props)
                if skip_object_spreads
                    && dir.arg.is_none()
                    && (dir.name == "bind" || dir.name == "on")
                {
                    continue;
                }
                // Only add comma if directive produces valid output
                if is_supported_directive(dir) {
                    // Check for duplicate v-on events that should be merged into arrays
                    if dir.name == "on" {
                        if let Some(event_key) = get_von_event_key(dir) {
                            let count = event_counts.get(&event_key).copied().unwrap_or(0);
                            if count > 1 {
                                if emitted_events.contains(&event_key) {
                                    // Skip: already emitted as part of array
                                    continue;
                                }
                                // First occurrence: emit as array with all handlers for this event
                                emitted_events.insert(event_key.clone());
                                if !first {
                                    ctx.push(",");
                                }
                                if multiline {
                                    ctx.newline();
                                } else if !first {
                                    ctx.push(" ");
                                }
                                first = false;
                                generate_merged_event_handlers(
                                    ctx,
                                    props,
                                    &event_key,
                                    static_class,
                                    static_style,
                                );
                                continue;
                            }
                        }
                    }

                    if !first {
                        ctx.push(",");
                    }
                    if multiline {
                        ctx.newline();
                    } else if !first {
                        ctx.push(" ");
                    }
                    first = false;
                    generate_directive_prop_with_static(ctx, dir, static_class, static_style);
                }
            }
        }
    }

    // Add scope_id attribute for scoped CSS
    if let Some(ref sid) = scope_id {
        if !first {
            ctx.push(",");
        }
        if multiline {
            ctx.newline();
        } else if !first {
            ctx.push(" ");
        }
        ctx.push("\"");
        ctx.push(sid);
        ctx.push("\": \"\"");
    }

    if multiline {
        ctx.deindent();
        ctx.newline();
        ctx.push("}");
    } else {
        ctx.push(" }");
    }

    // Restore skip_normalize flag
    ctx.skip_normalize = prev_skip;
}
