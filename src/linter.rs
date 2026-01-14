use std::collections::{HashMap, HashSet};

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::{Parser as OxcParser, ParserReturn};
use oxc_semantic::{Semantic, SemanticBuilder, SymbolId};
use oxc_span::{GetSpan, SourceType, Span};

#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub rule: String,
}

impl LintDiagnostic {
    pub fn new(file: &str, source: &str, span: Span, message: &str, rule: &str) -> Self {
        let (line, column) = offset_to_line_col(source, span.start as usize);
        Self {
            file: file.to_string(),
            line,
            column,
            message: message.to_string(),
            rule: rule.to_string(),
        }
    }
}

fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.chars().enumerate() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Context for tracking when we're inside a step.do callback
#[derive(Debug, Clone)]
struct StepCallbackContext {
    method_name: String, // e.g., "step.do"
}

/// Tracks step promise calls within a function scope
#[derive(Debug, Default)]
struct StepPromiseTracker {
    /// Maps variable names to the span of the step call they were assigned from
    /// e.g., `const p = step.do(...)` maps "p" -> span of step.do call
    var_to_step_span: HashMap<String, Span>,
    /// Maps step call spans to their method name (for error reporting)
    step_span_to_name: HashMap<Span, String>,
    /// Set of step call spans that have been awaited (directly or via Promise.all/race/etc.)
    awaited_step_spans: HashSet<Span>,
    /// Step calls that were not assigned to a variable and not immediately awaited
    unassigned_unawaited_steps: Vec<(Span, String)>,
}

impl StepPromiseTracker {
    fn new() -> Self {
        Self::default()
    }

    /// Record a step call that was assigned to a variable
    fn record_assigned_step(&mut self, var_name: &str, span: Span, method_name: String) {
        self.var_to_step_span.insert(var_name.to_string(), span);
        self.step_span_to_name.insert(span, method_name);
    }

    /// Record a step call that was NOT assigned to a variable and NOT immediately awaited
    fn record_unassigned_unawaited_step(&mut self, span: Span, method_name: String) {
        self.unassigned_unawaited_steps.push((span, method_name));
    }

    /// Mark a step call as awaited by its span
    fn mark_awaited_by_span(&mut self, span: Span) {
        self.awaited_step_spans.insert(span);
    }

    /// Mark a step call as awaited by variable name
    fn mark_awaited_by_var(&mut self, var_name: &str) {
        if let Some(&span) = self.var_to_step_span.get(var_name) {
            self.awaited_step_spans.insert(span);
        }
    }

    /// Get all step calls that were not awaited
    fn get_unawaited_steps(&self) -> Vec<(Span, String)> {
        let mut result = Vec::new();

        // Check assigned step calls
        for (var_name, &span) in &self.var_to_step_span {
            if !self.awaited_step_spans.contains(&span) {
                if let Some(method_name) = self.step_span_to_name.get(&span) {
                    result.push((span, method_name.clone()));
                } else {
                    result.push((span, format!("step (var: {})", var_name)));
                }
            }
        }

        // Add unassigned unawaited steps
        result.extend(self.unassigned_unawaited_steps.clone());

        result
    }
}

/// Tracks which symbols are known to be WorkflowStep instances
#[derive(Debug, Default)]
struct WorkflowStepSymbols {
    /// SymbolIds of parameters explicitly typed as WorkflowStep (TypeScript only)
    typed_step_symbols: HashSet<SymbolId>,
    /// SymbolIds of 2nd params of run() methods in classes extending WorkflowEntrypoint
    /// (JavaScript heuristic - also applies to TypeScript)
    inferred_step_symbols: HashSet<SymbolId>,
}

impl WorkflowStepSymbols {
    fn contains(&self, symbol_id: SymbolId) -> bool {
        self.typed_step_symbols.contains(&symbol_id)
            || self.inferred_step_symbols.contains(&symbol_id)
    }
}

/// Tracks imports from "cloudflare:workers"
/// Used only for JavaScript heuristic (detecting classes that extend WorkflowEntrypoint)
#[derive(Debug, Default)]
struct CloudflareImports {
    /// Maps local name -> imported name for imports from "cloudflare:workers"
    /// e.g., if `import { WorkflowEntrypoint as WE } from "cloudflare:workers"`,
    /// then local_to_imported["WE"] = "WorkflowEntrypoint"
    local_to_imported: HashMap<String, String>,
    /// SymbolIds of imported items from "cloudflare:workers"
    symbol_ids: HashMap<String, SymbolId>,
}

impl CloudflareImports {
    fn is_workflow_entrypoint(&self, local_name: &str) -> bool {
        self.local_to_imported
            .get(local_name)
            .map(|imported| imported == "WorkflowEntrypoint")
            .unwrap_or(false)
    }

    fn get_workflow_entrypoint_symbol(&self) -> Option<SymbolId> {
        for (local_name, imported_name) in &self.local_to_imported {
            if imported_name == "WorkflowEntrypoint" {
                return self.symbol_ids.get(local_name).copied();
            }
        }
        None
    }
}

/// Find all imports from "cloudflare:workers"
/// Used for JavaScript heuristic (detecting WorkflowEntrypoint class)
fn find_cloudflare_imports(program: &Program) -> CloudflareImports {
    let mut imports = CloudflareImports::default();

    for stmt in &program.body {
        if let Statement::ImportDeclaration(import_decl) = stmt {
            let source = import_decl.source.value.as_str();
            if source == "cloudflare:workers" {
                if let Some(specifiers) = &import_decl.specifiers {
                    for specifier in specifiers {
                        if let ImportDeclarationSpecifier::ImportSpecifier(spec) = specifier {
                            let local_name = spec.local.name.as_str().to_string();
                            let imported_name = spec.imported.name().as_str().to_string();
                            imports
                                .local_to_imported
                                .insert(local_name.clone(), imported_name);

                            // Get the SymbolId for this import
                            if let Some(symbol_id) = spec.local.symbol_id.get() {
                                imports.symbol_ids.insert(local_name, symbol_id);
                            }
                        }
                    }
                }
            }
        }
    }

    imports
}

/// Check if a type annotation refers to WorkflowStep
/// For TypeScript, we simply check if the type name is "WorkflowStep"
/// (the type comes from @cloudflare/workers-types ambient declarations)
fn is_workflow_step_type(type_ann: &TSTypeAnnotation) -> bool {
    if let TSType::TSTypeReference(type_ref) = &type_ann.type_annotation {
        if let TSTypeName::IdentifierReference(id) = &type_ref.type_name {
            return id.name.as_str() == "WorkflowStep";
        }
    }
    false
}

/// Find all parameters typed as WorkflowStep (TypeScript only)
/// Simply looks for any parameter with type annotation "WorkflowStep"
fn find_typed_workflow_step_symbols(program: &Program) -> HashSet<SymbolId> {
    let mut symbols = HashSet::new();

    // Helper to process function parameters
    fn process_params(params: &FormalParameters, symbols: &mut HashSet<SymbolId>) {
        for param in &params.items {
            // type_annotation is directly on FormalParameter
            if let Some(type_ann) = &param.type_annotation {
                if is_workflow_step_type(type_ann) {
                    // Get the symbol id from the binding pattern
                    // BindingPattern is an enum - use get_binding_identifier()
                    if let Some(id) = param.pattern.get_binding_identifier() {
                        if let Some(symbol_id) = id.symbol_id.get() {
                            symbols.insert(symbol_id);
                        }
                    }
                }
            }
        }
    }

    // Walk the AST to find all functions with WorkflowStep typed parameters
    for stmt in &program.body {
        match stmt {
            Statement::FunctionDeclaration(func) => {
                process_params(&func.params, &mut symbols);
            }
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::FunctionDeclaration(func) = &export.declaration
                {
                    process_params(&func.params, &mut symbols);
                }
                if let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration {
                    process_class_methods(class, &mut symbols);
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::FunctionDeclaration(func)) = &export.declaration {
                    process_params(&func.params, &mut symbols);
                }
                if let Some(Declaration::ClassDeclaration(class)) = &export.declaration {
                    process_class_methods(class, &mut symbols);
                }
            }
            Statement::ClassDeclaration(class) => {
                process_class_methods(class, &mut symbols);
            }
            Statement::VariableDeclaration(var_decl) => {
                for declarator in &var_decl.declarations {
                    if let Some(init) = &declarator.init {
                        process_expression_functions(init, &mut symbols);
                    }
                }
            }
            Statement::ExpressionStatement(expr_stmt) => {
                process_expression_functions(&expr_stmt.expression, &mut symbols);
            }
            _ => {}
        }
    }

    symbols
}

/// Process class methods to find WorkflowStep typed parameters
fn process_class_methods(class: &Class, symbols: &mut HashSet<SymbolId>) {
    for element in &class.body.body {
        if let ClassElement::MethodDefinition(method) = element {
            for param in &method.value.params.items {
                // type_annotation is directly on FormalParameter
                if let Some(type_ann) = &param.type_annotation {
                    if is_workflow_step_type(type_ann) {
                        // BindingPattern is an enum - use get_binding_identifier()
                        if let Some(id) = param.pattern.get_binding_identifier() {
                            if let Some(symbol_id) = id.symbol_id.get() {
                                symbols.insert(symbol_id);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Process function expressions (arrow functions, function expressions) for WorkflowStep params
fn process_expression_functions(expr: &Expression, symbols: &mut HashSet<SymbolId>) {
    match expr {
        Expression::ArrowFunctionExpression(arrow) => {
            for param in &arrow.params.items {
                // type_annotation is directly on FormalParameter
                if let Some(type_ann) = &param.type_annotation {
                    if is_workflow_step_type(type_ann) {
                        // BindingPattern is an enum - use get_binding_identifier()
                        if let Some(id) = param.pattern.get_binding_identifier() {
                            if let Some(symbol_id) = id.symbol_id.get() {
                                symbols.insert(symbol_id);
                            }
                        }
                    }
                }
            }
        }
        Expression::FunctionExpression(func) => {
            for param in &func.params.items {
                // type_annotation is directly on FormalParameter
                if let Some(type_ann) = &param.type_annotation {
                    if is_workflow_step_type(type_ann) {
                        // BindingPattern is an enum - use get_binding_identifier()
                        if let Some(id) = param.pattern.get_binding_identifier() {
                            if let Some(symbol_id) = id.symbol_id.get() {
                                symbols.insert(symbol_id);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Find inferred WorkflowStep symbols (2nd param of run() in classes extending WorkflowEntrypoint)
/// This works for both JavaScript and TypeScript
fn find_inferred_workflow_step_symbols(
    program: &Program,
    semantic: &Semantic,
    cloudflare_imports: &CloudflareImports,
) -> HashSet<SymbolId> {
    let mut symbols = HashSet::new();

    // Get the SymbolId for WorkflowEntrypoint import (if any)
    let workflow_entrypoint_symbol = cloudflare_imports.get_workflow_entrypoint_symbol();

    // Helper to check if a class extends WorkflowEntrypoint
    fn extends_workflow_entrypoint(
        class: &Class,
        semantic: &Semantic,
        cloudflare_imports: &CloudflareImports,
        workflow_entrypoint_symbol: Option<SymbolId>,
    ) -> bool {
        if let Some(super_class) = &class.super_class {
            if let Expression::Identifier(id) = super_class {
                // Check by name first
                if cloudflare_imports.is_workflow_entrypoint(id.name.as_str()) {
                    return true;
                }

                // Also check by symbol resolution
                if let Some(expected_symbol) = workflow_entrypoint_symbol {
                    if let Some(reference_id) = id.reference_id.get() {
                        let reference = semantic.scoping().get_reference(reference_id);
                        if let Some(symbol_id) = reference.symbol_id() {
                            return symbol_id == expected_symbol;
                        }
                    }
                }
            }
        }
        false
    }

    // Helper to get the 2nd parameter's SymbolId from a run() method
    fn get_run_method_step_param(class: &Class) -> Option<SymbolId> {
        for element in &class.body.body {
            if let ClassElement::MethodDefinition(method) = element {
                // Check if this is the "run" method
                if let PropertyKey::StaticIdentifier(id) = &method.key {
                    if id.name.as_str() == "run" {
                        // Get the 2nd parameter (index 1)
                        if let Some(param) = method.value.params.items.get(1) {
                            // BindingPattern is an enum - use get_binding_identifier()
                            if let Some(id) = param.pattern.get_binding_identifier() {
                                return id.symbol_id.get();
                            }
                        }
                    }
                }
            }
        }
        None
    }

    // Walk the AST to find classes extending WorkflowEntrypoint
    for stmt in &program.body {
        let class = match stmt {
            Statement::ClassDeclaration(class) => Some(class.as_ref()),
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration {
                    Some(class.as_ref())
                } else {
                    None
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::ClassDeclaration(class)) = &export.declaration {
                    Some(class.as_ref())
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(class) = class {
            if extends_workflow_entrypoint(
                class,
                semantic,
                cloudflare_imports,
                workflow_entrypoint_symbol,
            ) {
                if let Some(symbol_id) = get_run_method_step_param(class) {
                    symbols.insert(symbol_id);
                }
            }
        }
    }

    symbols
}

/// Build the complete WorkflowStepSymbols from the program
fn build_workflow_step_symbols(program: &Program, semantic: &Semantic) -> WorkflowStepSymbols {
    // For TypeScript: find parameters typed as WorkflowStep (no import needed)
    let typed_step_symbols = find_typed_workflow_step_symbols(program);

    // For JavaScript: find 2nd param of run() in classes extending WorkflowEntrypoint
    // This requires tracking imports from "cloudflare:workers"
    let cloudflare_imports = find_cloudflare_imports(program);
    let inferred_step_symbols =
        find_inferred_workflow_step_symbols(program, semantic, &cloudflare_imports);

    WorkflowStepSymbols {
        typed_step_symbols,
        inferred_step_symbols,
    }
}

struct Linter<'a> {
    source: &'a str,
    file_path: &'a str,
    diagnostics: Vec<LintDiagnostic>,
    /// Stack of trackers for nested function scopes
    tracker_stack: Vec<StepPromiseTracker>,
    /// Stack for tracking when we're inside a step.do callback (for nested-step rule)
    step_callback_stack: Vec<StepCallbackContext>,
    /// Semantic model for symbol resolution
    semantic: &'a Semantic<'a>,
    /// Known WorkflowStep symbols
    workflow_step_symbols: WorkflowStepSymbols,
}

impl<'a> Linter<'a> {
    fn new(
        source: &'a str,
        file_path: &'a str,
        semantic: &'a Semantic<'a>,
        workflow_step_symbols: WorkflowStepSymbols,
    ) -> Self {
        Self {
            source,
            file_path,
            diagnostics: Vec::new(),
            tracker_stack: Vec::new(),
            step_callback_stack: Vec::new(),
            semantic,
            workflow_step_symbols,
        }
    }

    fn current_tracker(&mut self) -> Option<&mut StepPromiseTracker> {
        self.tracker_stack.last_mut()
    }

    fn push_tracker(&mut self) {
        self.tracker_stack.push(StepPromiseTracker::new());
    }

    fn pop_tracker_and_report(&mut self) {
        if let Some(tracker) = self.tracker_stack.pop() {
            for (span, method_name) in tracker.get_unawaited_steps() {
                self.diagnostics.push(LintDiagnostic::new(
                    self.file_path,
                    self.source,
                    span,
                    &format!(
                        "`{}` must be awaited. Not awaiting creates a dangling Promise that can cause race conditions and swallowed errors.",
                        method_name
                    ),
                    "await-step",
                ));
            }
        }
    }

    fn lint_program(&mut self, program: &Program) {
        // Push a tracker for the top-level scope
        self.push_tracker();
        for stmt in &program.body {
            self.lint_statement(stmt);
        }
        self.pop_tracker_and_report();
    }

    fn lint_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::ExpressionStatement(expr_stmt) => {
                self.lint_expression(&expr_stmt.expression, false);
            }
            Statement::VariableDeclaration(decl) => {
                self.lint_variable_declaration(decl);
            }
            Statement::FunctionDeclaration(func) => {
                self.lint_function_body(func.body.as_deref());
            }
            Statement::ClassDeclaration(class) => {
                self.lint_class(class);
            }
            Statement::BlockStatement(block) => {
                for s in &block.body {
                    self.lint_statement(s);
                }
            }
            Statement::IfStatement(if_stmt) => {
                self.lint_expression(&if_stmt.test, false);
                self.lint_statement(&if_stmt.consequent);
                if let Some(alt) = &if_stmt.alternate {
                    self.lint_statement(alt);
                }
            }
            Statement::WhileStatement(while_stmt) => {
                self.lint_expression(&while_stmt.test, false);
                self.lint_statement(&while_stmt.body);
            }
            Statement::ForStatement(for_stmt) => {
                if let Some(init) = &for_stmt.init {
                    if let ForStatementInit::VariableDeclaration(decl) = init {
                        self.lint_variable_declaration(decl);
                    }
                }
                self.lint_statement(&for_stmt.body);
            }
            Statement::ForInStatement(for_in) => {
                self.lint_statement(&for_in.body);
            }
            Statement::ForOfStatement(for_of) => {
                self.lint_expression(&for_of.right, false);
                self.lint_statement(&for_of.body);
            }
            Statement::ReturnStatement(ret) => {
                if let Some(arg) = &ret.argument {
                    self.lint_expression(arg, false);
                }
            }
            Statement::TryStatement(try_stmt) => {
                for s in &try_stmt.block.body {
                    self.lint_statement(s);
                }
                if let Some(handler) = &try_stmt.handler {
                    for s in &handler.body.body {
                        self.lint_statement(s);
                    }
                }
                if let Some(finalizer) = &try_stmt.finalizer {
                    for s in &finalizer.body {
                        self.lint_statement(s);
                    }
                }
            }
            Statement::SwitchStatement(switch) => {
                self.lint_expression(&switch.discriminant, false);
                for case in &switch.cases {
                    for s in &case.consequent {
                        self.lint_statement(s);
                    }
                }
            }
            Statement::ExportDefaultDeclaration(export) => match &export.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    self.lint_function_body(func.body.as_deref());
                }
                ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    self.lint_class(class);
                }
                _ => {
                    if let Some(expr) = export.declaration.as_expression() {
                        self.lint_expression(expr, false);
                    }
                }
            },
            Statement::ExportNamedDeclaration(export) => {
                if let Some(decl) = &export.declaration {
                    self.lint_declaration(decl);
                }
            }
            _ => {}
        }
    }

    fn lint_declaration(&mut self, decl: &Declaration) {
        match decl {
            Declaration::FunctionDeclaration(func) => {
                self.lint_function_body(func.body.as_deref());
            }
            Declaration::ClassDeclaration(class) => {
                self.lint_class(class);
            }
            Declaration::VariableDeclaration(var_decl) => {
                self.lint_variable_declaration(var_decl);
            }
            _ => {}
        }
    }

    fn lint_variable_declaration(&mut self, decl: &VariableDeclaration) {
        for declarator in &decl.declarations {
            if let Some(init) = &declarator.init {
                // Check if initializer is a step call
                if let Expression::CallExpression(call) = init {
                    if self.is_step_method_call(call) {
                        // Get the variable name being assigned to
                        // BindingPattern is an enum - use get_binding_identifier()
                        if let Some(id) = declarator.id.get_binding_identifier() {
                            let var_name = id.name.as_str();
                            let method_name = self.get_step_method_name(call);
                            if let Some(tracker) = self.current_tracker() {
                                tracker.record_assigned_step(var_name, call.span(), method_name);
                            }
                        }
                        // Still lint the call's arguments
                        self.lint_call_arguments(call);
                        continue;
                    }
                }
                // Normal case: lint the initializer
                self.lint_expression(init, false);
            }
        }
    }

    fn lint_class(&mut self, class: &Class) {
        for element in &class.body.body {
            match element {
                ClassElement::MethodDefinition(method) => {
                    self.lint_function_body(method.value.body.as_deref());
                }
                ClassElement::PropertyDefinition(prop) => {
                    if let Some(value) = &prop.value {
                        self.lint_expression(value, false);
                    }
                }
                ClassElement::StaticBlock(block) => {
                    for s in &block.body {
                        self.lint_statement(s);
                    }
                }
                _ => {}
            }
        }
    }

    fn lint_function_body(&mut self, body: Option<&FunctionBody>) {
        if let Some(body) = body {
            self.push_tracker();
            for stmt in &body.statements {
                self.lint_statement(stmt);
            }
            self.pop_tracker_and_report();
        }
    }

    /// Helper to lint only the arguments of a call expression
    fn lint_call_arguments(&mut self, call: &CallExpression) {
        for arg in &call.arguments {
            if let Argument::SpreadElement(spread) = arg {
                self.lint_expression(&spread.argument, false);
            } else if let Some(expr) = arg.as_expression() {
                self.lint_expression(expr, false);
            }
        }
    }

    /// Check if a call is Promise.all, Promise.race, Promise.allSettled, or Promise.any
    fn is_promise_combinator_call(&self, call: &CallExpression) -> bool {
        if let Expression::StaticMemberExpression(member) = &call.callee {
            let method_name = member.property.name.as_str();
            if matches!(method_name, "all" | "race" | "allSettled" | "any") {
                if let Expression::Identifier(id) = &member.object {
                    return id.name.as_str() == "Promise";
                }
            }
        }
        false
    }

    /// Extract identifier names from an array expression (for Promise.all([a, b, c]))
    fn extract_identifiers_from_array(&self, arr: &ArrayExpression) -> Vec<String> {
        let mut identifiers = Vec::new();
        for elem in &arr.elements {
            if let Some(expr) = elem.as_expression() {
                if let Expression::Identifier(id) = expr {
                    identifiers.push(id.name.to_string());
                }
            }
        }
        identifiers
    }

    /// Mark step promises as awaited when encountering await expressions
    fn handle_await_expression(&mut self, await_expr: &AwaitExpression) {
        let arg = &await_expr.argument;

        // Case 1: await identifier (e.g., await p)
        if let Expression::Identifier(id) = arg {
            if let Some(tracker) = self.current_tracker() {
                tracker.mark_awaited_by_var(id.name.as_str());
            }
        }

        // Case 2: await Promise.all([...]) / Promise.race([...]) / etc.
        if let Expression::CallExpression(call) = arg {
            if self.is_promise_combinator_call(call) {
                // Check first argument for array of promises
                if let Some(first_arg) = call.arguments.first() {
                    if let Some(Expression::ArrayExpression(arr)) = first_arg.as_expression() {
                        let identifiers = self.extract_identifiers_from_array(arr);
                        if let Some(tracker) = self.current_tracker() {
                            for var_name in identifiers {
                                tracker.mark_awaited_by_var(&var_name);
                            }
                        }
                    }
                }
            }
        }
    }

    fn lint_expression(&mut self, expr: &Expression, is_awaited: bool) {
        match expr {
            Expression::AwaitExpression(await_expr) => {
                // Handle marking step promises as awaited
                self.handle_await_expression(await_expr);
                // The argument of await IS awaited
                self.lint_expression(&await_expr.argument, true);
            }
            Expression::CallExpression(call) => {
                // Check if this is a step.do or step.sleep call
                if self.is_step_method_call(call) {
                    let method_name = self.get_step_method_name(call);

                    // Check for nested step calls (nested-step rule)
                    if let Some(outer_ctx) = self.step_callback_stack.last() {
                        self.diagnostics.push(LintDiagnostic::new(
                            self.file_path,
                            self.source,
                            call.span(),
                            &format!(
                                "`{}` is nested inside `{}`. Nested steps are discouraged as they can cause unexpected behavior during workflow replay.",
                                method_name,
                                outer_ctx.method_name
                            ),
                            "nested-step",
                        ));
                    }

                    if is_awaited {
                        // Immediately awaited - mark as awaited by span
                        if let Some(tracker) = self.current_tracker() {
                            tracker.mark_awaited_by_span(call.span());
                        }
                    } else {
                        // Not immediately awaited and not in a variable assignment
                        // Record as unassigned unawaited step
                        if let Some(tracker) = self.current_tracker() {
                            tracker
                                .record_unassigned_unawaited_step(call.span(), method_name.clone());
                        }
                    }

                    // Handle step.do callback specially for nested step detection
                    if self.is_step_do_call(call) {
                        self.lint_step_do_with_callback(call, &method_name);
                    } else {
                        // For non-step.do calls, just lint arguments normally
                        self.lint_call_arguments(call);
                    }
                    return;
                }

                // Special case: if this is an awaited Promise.all/race/etc, treat array contents as awaited
                if is_awaited && self.is_promise_combinator_call(call) {
                    self.lint_expression(&call.callee, false);
                    // Lint array argument with is_awaited=true so step calls inside are treated as awaited
                    if let Some(first_arg) = call.arguments.first() {
                        if let Some(expr) = first_arg.as_expression() {
                            self.lint_expression(expr, true);
                        }
                    }
                } else {
                    // Lint the callee and arguments normally
                    self.lint_expression(&call.callee, false);
                    self.lint_call_arguments(call);
                }
            }
            Expression::ArrowFunctionExpression(arrow) => {
                self.push_tracker();
                for stmt in &arrow.body.statements {
                    self.lint_statement(stmt);
                }
                self.pop_tracker_and_report();
            }
            Expression::FunctionExpression(func) => {
                self.lint_function_body(func.body.as_deref());
            }
            Expression::ClassExpression(class) => {
                self.lint_class(class);
            }
            Expression::ArrayExpression(arr) => {
                // Propagate is_awaited to array elements (for Promise.all([step.x(), step.y()]))
                for elem in &arr.elements {
                    match elem {
                        ArrayExpressionElement::SpreadElement(spread) => {
                            self.lint_expression(&spread.argument, is_awaited);
                        }
                        _ => {
                            if let Some(expr) = elem.as_expression() {
                                self.lint_expression(expr, is_awaited);
                            }
                        }
                    }
                }
            }
            Expression::ObjectExpression(obj) => {
                for prop in &obj.properties {
                    match prop {
                        ObjectPropertyKind::ObjectProperty(p) => {
                            self.lint_expression(&p.value, false);
                        }
                        ObjectPropertyKind::SpreadProperty(spread) => {
                            self.lint_expression(&spread.argument, false);
                        }
                    }
                }
            }
            Expression::ConditionalExpression(cond) => {
                self.lint_expression(&cond.test, false);
                self.lint_expression(&cond.consequent, is_awaited);
                self.lint_expression(&cond.alternate, is_awaited);
            }
            Expression::BinaryExpression(bin) => {
                self.lint_expression(&bin.left, false);
                self.lint_expression(&bin.right, false);
            }
            Expression::LogicalExpression(log) => {
                self.lint_expression(&log.left, false);
                self.lint_expression(&log.right, false);
            }
            Expression::AssignmentExpression(assign) => {
                self.lint_expression(&assign.right, false);
            }
            Expression::SequenceExpression(seq) => {
                for (i, expr) in seq.expressions.iter().enumerate() {
                    // Only the last expression in a sequence can be awaited
                    let last = i == seq.expressions.len() - 1;
                    self.lint_expression(expr, last && is_awaited);
                }
            }
            Expression::ParenthesizedExpression(paren) => {
                self.lint_expression(&paren.expression, is_awaited);
            }
            Expression::UnaryExpression(unary) => {
                self.lint_expression(&unary.argument, false);
            }
            Expression::NewExpression(new_expr) => {
                self.lint_expression(&new_expr.callee, false);
                for arg in &new_expr.arguments {
                    if let Some(expr) = arg.as_expression() {
                        self.lint_expression(expr, false);
                    }
                }
            }
            Expression::StaticMemberExpression(member) => {
                self.lint_expression(&member.object, false);
            }
            Expression::ComputedMemberExpression(member) => {
                self.lint_expression(&member.object, false);
                self.lint_expression(&member.expression, false);
            }
            Expression::PrivateFieldExpression(member) => {
                self.lint_expression(&member.object, false);
            }
            Expression::TaggedTemplateExpression(tagged) => {
                self.lint_expression(&tagged.tag, false);
            }
            Expression::TemplateLiteral(template) => {
                for expr in &template.expressions {
                    self.lint_expression(expr, false);
                }
            }
            Expression::YieldExpression(yield_expr) => {
                if let Some(arg) = &yield_expr.argument {
                    self.lint_expression(arg, false);
                }
            }
            _ => {}
        }
    }

    /// Check if the call expression is a WorkflowStep method call (do, sleep, etc.)
    /// Uses semantic analysis to verify the object is actually a WorkflowStep
    fn is_step_method_call(&self, call: &CallExpression) -> bool {
        if let Expression::StaticMemberExpression(member) = &call.callee {
            let method_name = member.property.name.as_str();
            if matches!(method_name, "do" | "sleep" | "waitForEvent" | "sleepUntil") {
                if let Expression::Identifier(id) = &member.object {
                    // Use semantic resolution to check if this identifier refers to a WorkflowStep
                    if let Some(reference_id) = id.reference_id.get() {
                        let reference = self.semantic.scoping().get_reference(reference_id);
                        if let Some(symbol_id) = reference.symbol_id() {
                            return self.workflow_step_symbols.contains(symbol_id);
                        }
                    }
                }
            }
        }
        false
    }

    /// Get the method name for error reporting (e.g., "step.do" or "step.sleep")
    fn get_step_method_name(&self, call: &CallExpression) -> String {
        if let Expression::StaticMemberExpression(member) = &call.callee {
            let method_name = member.property.name.as_str();
            if let Expression::Identifier(id) = &member.object {
                return format!("{}.{}", id.name, method_name);
            }
            return format!("step.{}", method_name);
        }
        "step.do".to_string()
    }

    /// Check if this is specifically a step.do call (which has a callback)
    fn is_step_do_call(&self, call: &CallExpression) -> bool {
        if let Expression::StaticMemberExpression(member) = &call.callee {
            let method_name = member.property.name.as_str();
            if method_name == "do" {
                if let Expression::Identifier(id) = &member.object {
                    // Use semantic resolution
                    if let Some(reference_id) = id.reference_id.get() {
                        let reference = self.semantic.scoping().get_reference(reference_id);
                        if let Some(symbol_id) = reference.symbol_id() {
                            return self.workflow_step_symbols.contains(symbol_id);
                        }
                    }
                }
            }
        }
        false
    }

    /// Lint a step.do call, handling the callback specially for nested step detection
    fn lint_step_do_with_callback(&mut self, call: &CallExpression, method_name: &str) {
        // step.do signature: step.do(name, callback, options?)
        // The callback is the second argument (index 1)
        for (i, arg) in call.arguments.iter().enumerate() {
            if let Some(expr) = arg.as_expression() {
                if i == 1 {
                    // This is the callback argument - push context before linting
                    self.step_callback_stack.push(StepCallbackContext {
                        method_name: method_name.to_string(),
                    });
                    self.lint_expression(expr, false);
                    self.step_callback_stack.pop();
                } else {
                    // Other arguments (name, options) - lint normally
                    self.lint_expression(expr, false);
                }
            } else if let Argument::SpreadElement(spread) = arg {
                self.lint_expression(&spread.argument, false);
            }
        }
    }

    fn into_diagnostics(self) -> Vec<LintDiagnostic> {
        self.diagnostics
    }
}

pub fn lint_source(source: &str, file_path: &str) -> Vec<LintDiagnostic> {
    let source_type = SourceType::from_path(file_path).unwrap_or_default();
    let allocator = Allocator::default();
    let ParserReturn { program, .. } = OxcParser::new(&allocator, source, source_type).parse();

    // Build semantic model for symbol resolution
    let semantic_ret = SemanticBuilder::new().build(&program);
    let semantic = semantic_ret.semantic;

    // Find all WorkflowStep symbols (typed for TS, inferred for JS)
    let workflow_step_symbols = build_workflow_step_symbols(&program, &semantic);

    let mut linter = Linter::new(source, file_path, &semantic, workflow_step_symbols);
    linter.lint_program(&program);
    linter.into_diagnostics()
}
