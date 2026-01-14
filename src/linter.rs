use std::collections::{HashMap, HashSet};

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::{Parser as OxcParser, ParserReturn};
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

pub struct Linter<'a> {
    source: &'a str,
    file_path: &'a str,
    diagnostics: Vec<LintDiagnostic>,
    /// Stack of trackers for nested function scopes
    tracker_stack: Vec<StepPromiseTracker>,
}

impl<'a> Linter<'a> {
    pub fn new(source: &'a str, file_path: &'a str) -> Self {
        Self {
            source,
            file_path,
            diagnostics: Vec::new(),
            tracker_stack: Vec::new(),
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

    pub fn lint_program(&mut self, program: &Program) {
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
                        if let BindingPattern::BindingIdentifier(id) = &declarator.id {
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
                    if is_awaited {
                        // Immediately awaited - mark as awaited by span
                        if let Some(tracker) = self.current_tracker() {
                            tracker.mark_awaited_by_span(call.span());
                        }
                    } else {
                        // Not immediately awaited and not in a variable assignment
                        // Record as unassigned unawaited step
                        if let Some(tracker) = self.current_tracker() {
                            tracker.record_unassigned_unawaited_step(call.span(), method_name);
                        }
                    }
                    // Still lint the call's arguments
                    self.lint_call_arguments(call);
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

    /// Check if the call expression is a step.do() or step.sleep() call
    fn is_step_method_call(&self, call: &CallExpression) -> bool {
        if let Expression::StaticMemberExpression(member) = &call.callee {
            let method_name = member.property.name.as_str();
            if matches!(method_name, "do" | "sleep" | "waitForEvent" | "sleepUntil") {
                // Check if the object is named "step" (or ends with step-like pattern)
                if let Expression::Identifier(id) = &member.object {
                    let name = id.name.as_str().to_lowercase();
                    return name == "step" || name.ends_with("step");
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

    pub fn into_diagnostics(self) -> Vec<LintDiagnostic> {
        self.diagnostics
    }
}

pub fn lint_source(source: &str, file_path: &str) -> Vec<LintDiagnostic> {
    let source_type = SourceType::from_path(file_path).unwrap_or_default();
    let allocator = Allocator::default();
    let ParserReturn { program, .. } = OxcParser::new(&allocator, source, source_type).parse();

    let mut linter = Linter::new(source, file_path);
    linter.lint_program(&program);
    linter.into_diagnostics()
}
