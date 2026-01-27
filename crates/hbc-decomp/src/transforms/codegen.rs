// Code generation from IR to JavaScript-like output.

use crate::ir::Statement;

// Options for code generation.
#[derive(Debug, Clone)]
pub struct CodegenOptions {
    // Indentation string.
    pub indent: String,
    // Include block labels as comments.
    pub include_labels: bool,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            indent: "  ".to_string(),
            include_labels: false,
        }
    }
}

impl CodegenOptions {
    pub fn new() -> Self {
        Self::default()
    }
}

// Code generator.
pub struct Codegen {
    options: CodegenOptions,
    indent_level: usize,
}

impl Codegen {
    pub fn new(options: CodegenOptions) -> Self {
        Codegen { options, indent_level: 0 }
    }

    // Generate code for a list of statements.
    pub fn generate_statements(&mut self, statements: &[Statement]) -> String {
        let mut output = String::new();
        for stmt in statements {
            output.push_str(&self.generate_stmt(stmt));
        }
        output
    }

    fn generate_stmt(&mut self, stmt: &Statement) -> String {
        let indent = self.current_indent();
        match stmt {
            Statement::Expr(e) => format!("{indent}{e};\n"),
            Statement::Let { name, value, kind } => format!("{indent}{kind} {name} = {value};\n"),
            Statement::Assign { target, value } => format!("{indent}{target} = {value};\n"),
            Statement::Delete { target, result } => {
                if let Some(r) = result {
                    format!("{indent}r{r} = delete {target};\n")
                } else {
                    format!("{indent}delete {target};\n")
                }
            }
            Statement::Break(label) => {
                if let Some(l) = label {
                    format!("{indent}break {l};\n")
                } else {
                    format!("{indent}break;\n")
                }
            }
            Statement::Continue(label) => {
                if let Some(l) = label {
                    format!("{indent}continue {l};\n")
                } else {
                    format!("{indent}continue;\n")
                }
            }
            Statement::Return(Some(e)) => format!("{indent}return {e};\n"),
            Statement::Return(None) => format!("{indent}return;\n"),
            Statement::Throw(e) => format!("{indent}throw {e};\n"),
            Statement::Debugger => format!("{indent}debugger;\n"),
            Statement::Comment(s) => format!("{indent}// {s}\n"),
            Statement::Goto(t) => format!("{indent}goto {t};\n"),
            Statement::CondGoto { condition, target, fallthrough } => {
                format!("{indent}if ({condition}) goto {target} else goto {fallthrough};\n")
            }
            Statement::If { condition, then_body, else_body } => {
                self.generate_if(condition, then_body, else_body)
            }
            Statement::While { condition, body } => {
                self.generate_while(condition, body)
            }
            Statement::DoWhile { body, condition } => {
                self.generate_do_while(body, condition)
            }
            Statement::For { init, condition, update, body } => {
                self.generate_for(init.as_deref(), condition.as_ref(), update.as_deref(), body)
            }
            Statement::ForOf { variable, iterable, body } => {
                let mut out = format!("{indent}for (const {variable} of {iterable}) {{\n");
                self.indent_level += 1;
                out.push_str(&self.generate_statements(body));
                self.indent_level -= 1;
                out.push_str(&format!("{indent}}}\n"));
                out
            }
            Statement::ForIn { variable, object, body } => {
                let mut out = format!("{indent}for (const {variable} in {object}) {{\n");
                self.indent_level += 1;
                out.push_str(&self.generate_statements(body));
                self.indent_level -= 1;
                out.push_str(&format!("{indent}}}\n"));
                out
            }
            Statement::Switch { discriminant, cases, default } => {
                let mut out = format!("{indent}switch ({discriminant}) {{\n");
                self.indent_level += 1;
                let case_indent = self.current_indent();
                
                for (val, body) in cases {
                    out.push_str(&format!("{case_indent}case {val}:\n"));
                    self.indent_level += 1;
                    out.push_str(&self.generate_statements(body));
                    self.indent_level -= 1;
                    // Auto-insert break if needed? For now we assume body flows correctly or we accept fallthrough
                    // But in reconstruction we usually want breaks.
                    // If the body doesn't end in return/break/throw/continue, we might want to add break?
                    // Let's check last statement.
                    if let Some(last) = body.last() {
                         match last {
                             Statement::Return(_) | Statement::Throw(_) | Statement::Goto(_) | Statement::CondGoto{..} => {},
                             Statement::Comment(c) if c == "break" || c == "continue" => {},
                             _ => {
                                 // Add break
                                 out.push_str(&format!("{}break;\n", self.current_indent()));
                             }
                         }
                    } else {
                         // Empty body needs break
                         out.push_str(&format!("{}break;\n", self.current_indent()));
                    }
                }
                
                if let Some(default_body) = default {
                    out.push_str(&format!("{case_indent}default:\n"));
                    self.indent_level += 1;
                    out.push_str(&self.generate_statements(default_body));
                    self.indent_level -= 1;
                }
                
                self.indent_level -= 1;
                out.push_str(&format!("{indent}}}\n"));
                out
            }
            Statement::TryCatch { try_body, catch_param, catch_body, finally_body } => {
                self.generate_try_catch(try_body, catch_param.as_deref(), catch_body, finally_body)
            }
            Statement::Block(stmts) => {
                let mut out = format!("{indent}{{\n");
                self.indent_level += 1;
                out.push_str(&self.generate_statements(stmts));
                self.indent_level -= 1;
                out.push_str(&format!("{}}}\n", self.current_indent()));
                out
            }
            Statement::Class { name, super_class, methods, .. } => {
                let mut out = format!("{indent}class {name}");
                if let Some(sc) = super_class {
                     out.push_str(&format!(" extends {sc}"));
                }
                out.push_str(" {\n");
                
                self.indent_level += 1;
                // Generate methods
                for method in methods {
                    let method_indent = self.current_indent();
                    if method.is_static {
                        out.push_str(&format!("{method_indent}static "));
                    } else {
                        out.push_str(&method_indent);
                    }

                    // Handle method kind (getter/setter)
                    let kind_prefix = match method.kind {
                        crate::ir::MethodKind::Getter => "get ",
                        crate::ir::MethodKind::Setter => "set ",
                        _ => "",
                    };

                    if let crate::ir::Expression::Function { is_async, is_generator, .. } = &method.value {
                        let async_prefix = if *is_async { "async " } else { "" };
                        let gen = if *is_generator { "*" } else { "" };

                        if let Some(body) = &method.body {
                             out.push_str(&format!("{kind_prefix}{async_prefix}{gen}{}() {{\n", method.key));
                             self.indent_level += 1;
                             out.push_str(&self.generate_statements(body));
                             self.indent_level -= 1;
                             out.push_str(&format!("{method_indent}}}\n"));
                        } else {
                             out.push_str(&format!("{kind_prefix}{async_prefix}{gen}{}() {{ /* compiled code */ }}\n", method.key));
                        }
                    } else {
                         // Fallback
                         out.push_str(&format!("{kind_prefix}{}() {{ ... }}\n", method.key));
                    }
                }
                
                self.indent_level -= 1;
                out.push_str(&format!("{indent}}}\n"));
                out
            }
        }
    }

    fn generate_if(
        &mut self,
        condition: &crate::ir::Expression,
        then_body: &[Statement],
        else_body: &[Statement],
    ) -> String {
        let indent = self.current_indent();
        let mut out = format!("{indent}if ({condition}) {{\n");

        self.indent_level += 1;
        out.push_str(&self.generate_statements(then_body));
        self.indent_level -= 1;

        if else_body.is_empty() {
            out.push_str(&format!("{indent}}}\n"));
        } else {
            out.push_str(&format!("{indent}}} else {{\n"));
            self.indent_level += 1;
            out.push_str(&self.generate_statements(else_body));
            self.indent_level -= 1;
            out.push_str(&format!("{indent}}}\n"));
        }

        out
    }

    fn generate_while(&mut self, condition: &crate::ir::Expression, body: &[Statement]) -> String {
        let indent = self.current_indent();
        let mut out = format!("{indent}while ({condition}) {{\n");

        self.indent_level += 1;
        out.push_str(&self.generate_statements(body));
        self.indent_level -= 1;

        out.push_str(&format!("{indent}}}\n"));
        out
    }

    fn generate_do_while(&mut self, body: &[Statement], condition: &crate::ir::Expression) -> String {
        let indent = self.current_indent();
        let mut out = format!("{indent}do {{\n");

        self.indent_level += 1;
        out.push_str(&self.generate_statements(body));
        self.indent_level -= 1;

        out.push_str(&format!("{indent}}} while ({condition});\n"));
        out
    }

    fn generate_for(
        &mut self,
        init: Option<&Statement>,
        condition: Option<&crate::ir::Expression>,
        update: Option<&Statement>,
        body: &[Statement],
    ) -> String {
        let indent = self.current_indent();

        // Format init (without newline and semicolon)
        let init_str = match init {
            Some(Statement::Assign { target, value }) => format!("{target} = {value}"),
            Some(Statement::Let { name, value, kind }) => format!("{kind} {name} = {value}"),
            _ => String::new(),
        };

        // Format condition
        let cond_str = condition.map(|c| format!("{c}")).unwrap_or_default();

        // Format update (without newline and semicolon)
        let update_str = match update {
            Some(Statement::Assign { target, value }) => format!("{target} = {value}"),
            Some(Statement::Expr(e)) => format!("{e}"),
            _ => String::new(),
        };

        let mut out = format!("{indent}for ({init_str}; {cond_str}; {update_str}) {{\n");

        self.indent_level += 1;
        out.push_str(&self.generate_statements(body));
        self.indent_level -= 1;

        out.push_str(&format!("{indent}}}\n"));
        out
    }

    fn generate_try_catch(
        &mut self,
        try_body: &[Statement],
        catch_param: Option<&str>,
        catch_body: &[Statement],
        finally_body: &[Statement],
    ) -> String {
        let indent = self.current_indent();
        let mut out = format!("{indent}try {{\n");

        self.indent_level += 1;
        out.push_str(&self.generate_statements(try_body));
        self.indent_level -= 1;

        if !catch_body.is_empty() || catch_param.is_some() {
            let param = catch_param.unwrap_or("e");
            out.push_str(&format!("{indent}}} catch ({param}) {{\n"));
            self.indent_level += 1;
            out.push_str(&self.generate_statements(catch_body));
            self.indent_level -= 1;
        }

        if !finally_body.is_empty() {
            out.push_str(&format!("{indent}}} finally {{\n"));
            self.indent_level += 1;
            out.push_str(&self.generate_statements(finally_body));
            self.indent_level -= 1;
        }

        out.push_str(&format!("{indent}}}\n"));
        out
    }

    fn current_indent(&self) -> String {
        self.options.indent.repeat(self.indent_level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Expression, Constant};

    #[test]
    fn test_simple_codegen() {
        let stmts = vec![
            Statement::let_stmt("x", Expression::constant(Constant::Integer(42))),
            Statement::Return(Some(Expression::Value(crate::ir::Value::Register(0)))),
        ];

        let mut codegen = Codegen::new(CodegenOptions::new());
        let output = codegen.generate_statements(&stmts);

        assert!(output.contains("let x = 42;"));
        assert!(output.contains("return r0;"));
    }

    #[test]
    fn test_if_codegen() {
        let stmts = vec![Statement::If {
            condition: Expression::Value(crate::ir::Value::Register(0)),
            then_body: vec![Statement::Return(Some(Expression::constant(Constant::Integer(1))))],
            else_body: vec![Statement::Return(Some(Expression::constant(Constant::Integer(0))))],
        }];

        let mut codegen = Codegen::new(CodegenOptions::new());
        let output = codegen.generate_statements(&stmts);

        assert!(output.contains("if (r0)"));
        assert!(output.contains("return 1;"));
        assert!(output.contains("return 0;"));
    }
}
