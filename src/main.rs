use std::path::Path;
use std::{env, fs};

use clap::Parser;
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::{Parser as OxcParser, ParserReturn};
use oxc_span::{GetSpan, SourceType, Span};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "cashmere")]
#[command(about = "A fast linter for Cloudflare Workflows TypeScript/JavaScript code, built with Rust.")]
struct Args {
    /// Directory or file to lint (defaults to current directory)
    #[arg(default_value = ".")]
    path: String,
}

#[derive(Debug)]
struct LintDiagnostic {
    file: String,
    line: usize,
    column: usize,
    message: String,
    rule: String,
}

impl LintDiagnostic {
    fn new(file: &str, source: &str, span: Span, message: &str, rule: &str) -> Self {
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

struct Linter<'a> {
    source: &'a str,
    file_path: &'a str,
    diagnostics: Vec<LintDiagnostic>,
}

impl<'a> Linter<'a> {
    fn new(source: &'a str, file_path: &'a str) -> Self {
        Self {
            source,
            file_path,
            diagnostics: Vec::new(),
        }
    }

    fn lint_program(&mut self, program: &Program) {
        for stmt in &program.body {
            self.lint_statement(stmt);
        }
    }

    fn lint_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::ExpressionStatement(expr_stmt) => {
                self.lint_expression(&expr_stmt.expression, false);
            }
            Statement::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    if let Some(init) = &declarator.init {
                        self.lint_expression(init, false);
                    }
                }
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
                        for declarator in &decl.declarations {
                            if let Some(init) = &declarator.init {
                                self.lint_expression(init, false);
                            }
                        }
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
            Statement::ExportDefaultDeclaration(export) => {
                match &export.declaration {
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
                }
            }
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
                for declarator in &var_decl.declarations {
                    if let Some(init) = &declarator.init {
                        self.lint_expression(init, false);
                    }
                }
            }
            _ => {}
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
            for stmt in &body.statements {
                self.lint_statement(stmt);
            }
        }
    }

    fn lint_expression(&mut self, expr: &Expression, is_awaited: bool) {
        match expr {
            Expression::AwaitExpression(await_expr) => {
                // The argument of await IS awaited
                self.lint_expression(&await_expr.argument, true);
            }
            Expression::CallExpression(call) => {
                // Check if this is a step.do or step.sleep call
                if self.is_step_method_call(call) && !is_awaited {
                    let method_name = self.get_step_method_name(call);
                    self.diagnostics.push(LintDiagnostic::new(
                        self.file_path,
                        self.source,
                        call.span(),
                        &format!(
                            "`{}` must be awaited. Not awaiting creates a dangling Promise that can cause race conditions and swallowed errors.",
                            method_name
                        ),
                        "await-step",
                    ));
                }

                // Lint the callee and arguments
                self.lint_expression(&call.callee, false);
                for arg in &call.arguments {
                    if let Argument::SpreadElement(spread) = arg {
                        self.lint_expression(&spread.argument, false);
                    } else if let Some(expr) = arg.as_expression() {
                        self.lint_expression(expr, false);
                    }
                }
            }
            Expression::ArrowFunctionExpression(arrow) => {
                for stmt in &arrow.body.statements {
                    self.lint_statement(stmt);
                }
            }
            Expression::FunctionExpression(func) => {
                self.lint_function_body(func.body.as_deref());
            }
            Expression::ClassExpression(class) => {
                self.lint_class(class);
            }
            Expression::ArrayExpression(arr) => {
                for elem in &arr.elements {
                    match elem {
                        ArrayExpressionElement::SpreadElement(spread) => {
                            self.lint_expression(&spread.argument, false);
                        }
                        _ => {
                            if let Some(expr) = elem.as_expression() {
                                self.lint_expression(expr, false);
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
            if method_name == "do" || method_name == "sleep" {
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
}

fn is_js_or_ts_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(ext, "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"),
        None => false,
    }
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | ".git" | "dist" | "build" | "target" | ".next" | "coverage"
    )
}

fn lint_file(path: &Path) -> Option<Vec<LintDiagnostic>> {
    let source_text = fs::read_to_string(path).ok()?;
    let source_type = SourceType::from_path(path).unwrap_or_default();

    let allocator = Allocator::default();
    let ParserReturn { program, .. } =
        OxcParser::new(&allocator, &source_text, source_type).parse();

    let mut linter = Linter::new(&source_text, path.to_str().unwrap_or(""));
    linter.lint_program(&program);

    Some(linter.diagnostics)
}

fn main() {
    let args = Args::parse();
    let root = if args.path == "." {
        env::current_dir().expect("Failed to get current directory")
    } else {
        Path::new(&args.path).to_path_buf()
    };

    let mut all_diagnostics: Vec<LintDiagnostic> = Vec::new();
    let mut files_checked = 0;

    if root.is_file() {
        if is_js_or_ts_file(&root) {
            if let Some(diagnostics) = lint_file(&root) {
                all_diagnostics.extend(diagnostics);
                files_checked += 1;
            }
        }
    } else {
        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|e| {
                if e.file_type().is_dir() {
                    !should_skip_dir(e.file_name().to_str().unwrap_or(""))
                } else {
                    true
                }
            })
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if path.is_file() && is_js_or_ts_file(path) {
                if let Some(diagnostics) = lint_file(path) {
                    all_diagnostics.extend(diagnostics);
                    files_checked += 1;
                }
            }
        }
    }

    // Print diagnostics
    for diagnostic in &all_diagnostics {
        println!(
            "{}:{}:{} - {} [{}]",
            diagnostic.file, diagnostic.line, diagnostic.column, diagnostic.message, diagnostic.rule
        );
    }

    // Print summary
    println!();
    if all_diagnostics.is_empty() {
        println!("✓ No issues found ({} files checked)", files_checked);
    } else {
        println!(
            "✗ Found {} issue(s) in {} file(s) checked",
            all_diagnostics.len(),
            files_checked
        );
        std::process::exit(1);
    }
}
