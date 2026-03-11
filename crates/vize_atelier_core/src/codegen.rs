//! VDom code generation.
//!
//! This module generates JavaScript render function code from the transformed AST.

mod children;
mod context;
mod element;
mod expression;
mod generate;
mod helpers;
mod node;
mod patch_flag;
mod props;
mod root;
mod slots;
mod v_for;
mod v_if;

use crate::{
    ast::{RootNode, RuntimeHelper, TemplateChildNode},
    options::CodegenOptions,
};

pub use context::{CodegenContext, CodegenResult};
use element::generate_root_node;
use generate::{collect_hoist_helpers, generate_hoists};
use node::generate_node;
use root::{
    generate_assets, generate_function_signature, generate_preamble_from_helpers,
    is_ignorable_root_text,
};

/// Generate code from root AST.
pub fn generate(root: &RootNode<'_>, options: CodegenOptions) -> CodegenResult {
    let mut ctx = CodegenContext::new(options);
    let root_children: std::vec::Vec<&TemplateChildNode<'_>> = root
        .children
        .iter()
        .filter(|child| !is_ignorable_root_text(child))
        .collect();

    // Generate function signature
    generate_function_signature(&mut ctx);

    // Generate body
    ctx.indent();
    ctx.newline();

    // Generate component/directive resolution
    generate_assets(&mut ctx, root);

    // Generate return statement
    ctx.push("return ");

    // Generate root node
    if root_children.is_empty() {
        ctx.push("null");
    } else if root_children.len() == 1 {
        // Single root child - wrap in block
        generate_root_node(&mut ctx, root_children[0]);
    } else {
        // Multiple root children - wrap in fragment block
        ctx.use_helper(RuntimeHelper::OpenBlock);
        ctx.use_helper(RuntimeHelper::CreateElementBlock);
        ctx.use_helper(RuntimeHelper::Fragment);
        ctx.push("(");
        ctx.push(ctx.helper(RuntimeHelper::OpenBlock));
        ctx.push("(), ");
        ctx.push(ctx.helper(RuntimeHelper::CreateElementBlock));
        ctx.push("(");
        ctx.push(ctx.helper(RuntimeHelper::Fragment));
        ctx.push(", null, [");
        ctx.indent();
        for (i, child) in root_children.iter().enumerate() {
            if i > 0 {
                ctx.push(",");
            }
            ctx.newline();
            generate_node(&mut ctx, child);
        }
        ctx.deindent();
        ctx.newline();
        ctx.push("], 64 /* STABLE_FRAGMENT */))");
    }

    ctx.deindent();
    ctx.newline();
    ctx.push("}");

    // Now generate preamble after we know all used helpers
    // Only include specific helpers from root.helpers that are known to be
    // added during transform but not tracked during codegen (like Unref)
    // We don't merge ALL root.helpers because transform may add helpers that
    // get optimized away during codegen (e.g., createElementVNode -> createElementBlock)
    let mut all_helpers: Vec<RuntimeHelper> = ctx.used_helpers.iter().copied().collect();
    if root.helpers.contains(&RuntimeHelper::Unref) && !all_helpers.contains(&RuntimeHelper::Unref)
    {
        all_helpers.push(RuntimeHelper::Unref);
    }
    // Collect helpers from hoisted nodes - generate_hoists() takes &CodegenContext (immutable)
    // so helpers used in hoisted VNodes aren't tracked via use_helper(). Pre-scan them here.
    collect_hoist_helpers(root, &mut all_helpers);
    // Sort helpers for consistent output order
    all_helpers.sort();
    all_helpers.dedup();

    let mut preamble = generate_preamble_from_helpers(&ctx, &all_helpers);

    // Generate hoisted variable declarations (appended to preamble)
    let hoists_code = generate_hoists(&ctx, root);
    if !hoists_code.is_empty() {
        preamble.push('\n');
        preamble.push_str(&hoists_code);
    }

    CodegenResult {
        code: ctx.into_code(),
        preamble,
        map: None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{assert_codegen, compile};

    #[test]
    fn test_codegen_simple_element() {
        assert_codegen!("<div>hello</div>" => contains: [
            "_createElementBlock",
            "\"div\"",
            "\"hello\""
        ]);
    }

    #[test]
    fn test_codegen_interpolation() {
        // When prefix_identifiers is false (default), expressions are not prefixed with _ctx.
        assert_codegen!("<div>{{ msg }}</div>" => contains: [
            "_toDisplayString",
            "msg"
        ]);
    }

    #[test]
    fn test_codegen_with_props() {
        assert_codegen!(r#"<div id="app" class="container"></div>"# => contains: [
            "id: \"app\"",
            "class: \"container\""
        ]);
    }

    #[test]
    fn test_codegen_component() {
        assert_codegen!("<MyComponent />" => contains: [
            "_resolveComponent",
            "_createBlock",
            "_component_MyComponent"
        ]);
    }

    #[test]
    fn test_codegen_pascal_case_dynamic_component() {
        let result = compile!(r#"<Component :is="current" :active-class="klass" />"#);

        assert!(
            result.code.contains("_resolveDynamicComponent(current)"),
            "PascalCase dynamic component should use resolveDynamicComponent: {}",
            result.code
        );
        assert!(
            !result.code.contains("_component_Component"),
            "PascalCase dynamic component should not resolve Component as a normal component: {}",
            result.code
        );
        assert!(
            !result.preamble.contains("_resolveComponent"),
            "PascalCase dynamic component should not import resolveComponent: {}",
            result.preamble
        );
        assert!(
            !result.code.contains("is: current"),
            "Dynamic component should not keep the is prop in generated props: {}",
            result.code
        );
    }

    #[test]
    fn test_codegen_pascal_case_dynamic_component_inside_v_for() {
        let result =
            compile!(r#"<Component :is="item.component" v-for="item in items" :key="item.id" />"#);

        assert!(
            result
                .code
                .contains("_resolveDynamicComponent(item.component)"),
            "v-for dynamic component should use resolveDynamicComponent: {}",
            result.code
        );
        assert!(
            !result.code.contains("is: item.component"),
            "v-for dynamic component should not keep the is prop: {}",
            result.code
        );
        assert!(
            !result.code.contains("\"is\""),
            "v-for dynamic component patch flags should not track is: {}",
            result.code
        );
    }

    #[test]
    fn test_codegen_v_if_template_fragment_wraps_interpolation_in_text_vnode() {
        let result = compile!(
            r#"<p><template v-if="ready">{{ count }}</template><span v-if="pending">updating</span></p>"#
        );

        assert!(
            result
                .code
                .contains("_createTextVNode(_toDisplayString(count), 1 /* TEXT */)"),
            "template v-if fragment should wrap interpolation in a text vnode: {}",
            result.code
        );
        assert!(
            !result
                .code
                .contains("_createElementBlock(_Fragment, { key: 0 }, [ _toDisplayString(count) ]"),
            "template v-if fragment should not leave raw strings in fragment children: {}",
            result.code
        );
    }

    #[test]
    fn test_codegen_v_if_template_fragment_wraps_static_text_in_text_vnode() {
        let result = compile!(
            r#"<div><template v-if="ready">Found packages</template><span v-if="pending">updating</span></div>"#
        );

        assert!(
            result.code.contains("_createTextVNode(\"Found packages\")"),
            "template v-if fragment should wrap static text in a text vnode: {}",
            result.code
        );
        assert!(
            !result
                .code
                .contains("_createElementBlock(_Fragment, { key: 0 }, [ \"Found packages\" ]"),
            "template v-if fragment should not emit raw text entries inside fragment arrays: {}",
            result.code
        );
    }

    #[test]
    fn test_codegen_preamble_module() {
        use crate::options::CodegenMode;
        let options = super::CodegenOptions {
            mode: CodegenMode::Module,
            ..Default::default()
        };
        let result = compile!("<div>hello</div>", options);
        assert!(result.preamble.contains("import {"));
        assert!(result.preamble.contains("from \"vue\""));
    }

    #[test]
    fn test_codegen_v_model_on_component() {
        // v-model on component should expand to modelValue + onUpdate:modelValue
        assert_codegen!(r#"<MyComponent v-model="msg" />"# => contains: [
            "_createBlock",
            "_component_MyComponent",
            "modelValue:",
            "msg",
            "\"onUpdate:modelValue\":"
        ]);
    }

    #[test]
    fn test_codegen_v_model_with_arg() {
        // v-model:title should expand to title + onUpdate:title
        assert_codegen!(r#"<MyComponent v-model:title="pageTitle" />"# => contains: [
            "title:",
            "pageTitle",
            "\"onUpdate:title\":"
        ]);
    }

    #[test]
    fn test_codegen_v_model_on_input() {
        // v-model on input uses withDirectives + vModelText
        assert_codegen!(r#"<input v-model="inputValue" />"# => contains: [
            "_withDirectives",
            "_vModelText",
            "inputValue",
            "\"onUpdate:modelValue\":"
        ]);
    }

    #[test]
    fn test_codegen_v_model_with_other_props() {
        // v-model with other props should not produce comments
        let result = compile!(r#"<MonacoEditor v-model="source" :language="editorLanguage" />"#);
        // Should NOT contain /* v-model */
        assert!(
            !result.code.contains("/* v-model */"),
            "Should not contain v-model comment"
        );
        // Should contain the expanded props
        assert!(
            result.code.contains("modelValue:"),
            "Should have modelValue prop"
        );
        assert!(
            result.code.contains("\"onUpdate:modelValue\":"),
            "Should have onUpdate:modelValue prop"
        );
        assert!(
            result.code.contains("language:"),
            "Should have language prop"
        );
    }

    #[test]
    fn test_codegen_slot_fallback() {
        // Slot element with fallback content should include fallback function
        assert_codegen!(r#"<slot name="label">{{ label }}</slot>"# => contains: [
            "_renderSlot",
            "\"label\"",
            "{}"
        ]);
        // Check that the fallback function is present
        let result = compile!(r#"<slot name="label">{{ label }}</slot>"#);
        assert!(
            result.code.contains("() => ["),
            "Should have fallback function: {}",
            result.code
        );
        assert!(
            result.code.contains("_toDisplayString"),
            "Should have toDisplayString for interpolation: {}",
            result.code
        );
    }

    #[test]
    fn test_codegen_slot_without_fallback() {
        // Slot element without fallback should not have empty object or function
        let result = compile!(r#"<slot name="header"></slot>"#);
        assert!(
            result.code.contains("_renderSlot"),
            "Should have renderSlot"
        );
        assert!(result.code.contains("\"header\""), "Should have slot name");
        // Should not have fallback function
        assert!(
            !result.code.contains("() => ["),
            "Should not have fallback function for empty slot: {}",
            result.code
        );
    }

    #[test]
    fn test_codegen_conditional_slot_with_else_does_not_append_undefined() {
        let result = compile!(
            r#"<MyDialog>
  <template v-if="step === 1" #header>First</template>
  <template v-else #header>Second</template>
</MyDialog>"#
        );
        assert!(
            result.code.contains("_createSlots"),
            "conditional slots should use createSlots. Got:\n{}",
            result.code
        );
        assert!(
            !result.code.contains(": undefined ])")
                && !result.code.contains(": undefined ]")
                && !result.code.contains(": undefined ],"),
            "final else branch should not emit an extra undefined arm. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_codegen_default_slot_with_v_if_is_marked_dynamic() {
        let result = compile!(
            r#"<PageWithHeader>
  <div v-if="tab === 'overview'">Overview</div>
  <div v-else-if="tab === 'emojis'">Emojis</div>
  <div v-else>Charts</div>
</PageWithHeader>"#
        );

        assert!(
            result.code.contains("_: 2 /* DYNAMIC */"),
            "default slot with v-if should be marked dynamic. Got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("1024 /* DYNAMIC_SLOTS */"),
            "component using that slot should carry DYNAMIC_SLOTS. Got:\n{}",
            result.code
        );
        assert!(
            !result.code.contains("_createSlots"),
            "implicit default slot should stay in the normal slots object path. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_codegen_forwarded_default_slot_is_marked_forwarded() {
        let result = compile!(r#"<MkSwiper><slot /></MkSwiper>"#);

        assert!(
            result
                .code
                .contains("_renderSlot(_ctx.$slots, \"default\")"),
            "forwarded slot should render the incoming default slot. Got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("_: 3 /* FORWARDED */"),
            "forwarded slot should use the FORWARDED slot flag. Got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("1024 /* DYNAMIC_SLOTS */"),
            "forwarded slot should force component slot updates. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_codegen_v_if_branch_mixed_children_wrap_interpolations_in_text_vnodes() {
        let result = compile!(
            r#"<p v-if="speaker.affiliation || speaker.title">{{ speaker.affiliation }}<br v-if="speaker.affiliation && speaker.title" />{{ speaker.title }}</p>"#
        );

        assert!(
            result
                .code
                .contains("_createTextVNode(_toDisplayString(speaker.affiliation), 1 /* TEXT */)"),
            "expected first interpolation to be wrapped in createTextVNode. Got:\n{}",
            result.code
        );
        assert!(
            result
                .code
                .contains("_createTextVNode(_toDisplayString(speaker.title), 1 /* TEXT */)"),
            "expected second interpolation to be wrapped in createTextVNode. Got:\n{}",
            result.code
        );
        assert!(
            !result
                .code
                .contains("[_toDisplayString(speaker.affiliation),"),
            "expected v-if branch children array to avoid raw string entries. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_codegen_v_for_aliases_without_parentheses_stay_local() {
        use crate::options::{CodegenOptions, TransformOptions};
        use crate::parser::parse;
        use crate::transform::transform;
        use bumpalo::Bump;

        let allocator = Bump::new();
        let (mut root, _) = parse(
            &allocator,
            r#"<div><template v-for="item, index of items" :key="index"><UserCard :user="item" :data-index="index" /></template></div>"#,
        );

        transform(
            &allocator,
            &mut root,
            TransformOptions {
                prefix_identifiers: true,
                ..Default::default()
            },
            None,
        );

        let result = super::generate(
            &root,
            CodegenOptions {
                prefix_identifiers: true,
                ..Default::default()
            },
        );

        assert!(
            result
                .code
                .contains("_renderList(_ctx.items, (item, index) => {"),
            "expected split aliases in renderList callback, got:\n{}",
            result.code
        );
        assert!(
            !result.code.contains("_ctx.item.")
                && !result.code.contains("_ctx.item,")
                && !result.code.contains("_ctx.item)")
                && !result.code.contains("_ctx.item]"),
            "v-for value alias should stay local, got:\n{}",
            result.code
        );
        assert!(
            !result.code.contains("_ctx.index.")
                && !result.code.contains("_ctx.index,")
                && !result.code.contains("_ctx.index)")
                && !result.code.contains("_ctx.index]"),
            "v-for key/index alias should stay local, got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("user: item"),
            "component prop should reference local alias, got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_codegen_v_for_scope_handlers_are_not_cached() {
        use crate::options::{CodegenOptions, TransformOptions};
        use crate::parser::parse;
        use crate::transform::transform;
        use bumpalo::Bump;

        let allocator = Bump::new();
        let (mut root, _) = parse(
            &allocator,
            r#"<button v-for="tab in tabs" :key="tab.id" @click="select(tab)">{{ tab.label }}</button>"#,
        );

        transform(
            &allocator,
            &mut root,
            TransformOptions {
                prefix_identifiers: true,
                ..Default::default()
            },
            None,
        );

        let result = super::generate(
            &root,
            CodegenOptions {
                prefix_identifiers: true,
                cache_handlers: true,
                ..Default::default()
            },
        );

        assert!(
            !result.code.contains("_cache["),
            "v-for scoped handlers must not be cached, got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("_ctx.select(tab)"),
            "handler should keep the v-for alias local, got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("\"onClick\""),
            "non-cached scoped handler should still be tracked as a dynamic prop, got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_codegen_scoped_slot_params_stay_local_in_handlers() {
        use crate::options::{CodegenOptions, TransformOptions};
        use crate::parser::parse;
        use crate::transform::transform;
        use bumpalo::Bump;

        let allocator = Bump::new();
        let (mut root, _) = parse(
            &allocator,
            r#"<CommonPaginator>
  <template #default="{ item, index }">
    <button @click="showHistory(item)">{{ index }}</button>
    <button @click="() => edit(item.id)">{{ item.id }}</button>
  </template>
</CommonPaginator>"#,
        );

        transform(
            &allocator,
            &mut root,
            TransformOptions {
                prefix_identifiers: true,
                ..Default::default()
            },
            None,
        );

        let result = super::generate(
            &root,
            CodegenOptions {
                prefix_identifiers: true,
                cache_handlers: true,
                ..Default::default()
            },
        );

        assert!(
            result.code.contains("_ctx.showHistory(item)")
                || result.code.contains("_ctx.showHistory(item))"),
            "scoped slot item should stay local in direct handler, got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("() => _ctx.edit(item.id)")
                || result.code.contains("() => _ctx.edit(item.id))"),
            "scoped slot item should stay local in arrow handler, got:\n{}",
            result.code
        );
        assert!(
            result.code.contains("_toDisplayString(index)")
                || result.code.contains("toDisplayString(index)"),
            "scoped slot index should stay local in interpolation, got:\n{}",
            result.code
        );
        assert!(
            !result.code.contains("_ctx.item."),
            "scoped slot item should not be prefixed with _ctx, got:\n{}",
            result.code
        );
        assert!(
            !result.code.contains("_ctx.index"),
            "scoped slot index should not be prefixed with _ctx, got:\n{}",
            result.code
        );
        assert!(
            !result.code.contains("_cache["),
            "scoped slot handlers must not be cached, got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_codegen_escape_newline_in_attribute() {
        // Attribute values containing newlines should be properly escaped
        let result = compile!(
            r#"<div style="
            color: red;
            background: blue;
        "></div>"#
        );
        // Should have properly escaped newlines
        assert!(
            result.code.contains("\\n"),
            "Should escape newlines in attribute values. Got:\n{}",
            result.code
        );
        // Should NOT have raw newlines inside string literals
        assert!(
            !result.code.contains("style: \"\n"),
            "Should not have raw newlines in string. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_codegen_escape_special_chars_in_attribute() {
        // Attribute values should escape backslashes and quotes
        let result = compile!(r#"<div data-value="line1\nline2"></div>"#);
        // Backslash should be escaped
        assert!(
            result.code.contains(r#"\\n"#),
            "Should escape backslashes in attribute values. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_codegen_escape_multiline_style_attribute() {
        // Complex multiline style attribute (real-world case from Discord issue)
        let result = compile!(
            r#"<div style="
            display: flex;
            flex-direction: column;
        "></div>"#
        );
        // Should produce valid JavaScript
        assert!(
            result.code.contains("style:"),
            "Should have style property. Got:\n{}",
            result.code
        );
        // All newlines should be escaped
        let style_start = result.code.find("style:").unwrap_or(0);
        let code_after_style = &result.code[style_start..];
        // Find the string value - should not contain raw newlines
        if let Some(quote_pos) = code_after_style.find('"') {
            let remaining = &code_after_style[quote_pos + 1..];
            if let Some(end_quote) = remaining.find('"') {
                let style_value = &remaining[..end_quote];
                assert!(
                    !style_value.contains('\n'),
                    "Style value should not contain raw newlines. Got:\n{}",
                    style_value
                );
            }
        }
    }
}
