// Statement types for the IR.

mod display;

use super::{Expression, BlockId};

// A statement in the IR.
use serde::{Serialize, Deserialize};

/// Variable declaration kind (const, let, var).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VarKind {
    /// ES6 const declaration (immutable binding).
    Const,
    /// ES6 let declaration (block-scoped mutable).
    #[default]
    Let,
    /// ES5 var declaration (function-scoped, hoisted).
    Var,
}

impl std::fmt::Display for VarKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VarKind::Const => write!(f, "const"),
            VarKind::Let => write!(f, "let"),
            VarKind::Var => write!(f, "var"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    // Expression statement (side effects only).
    Expr(Expression),

    // Variable declaration: `let/const/var name = value`.
    Let { name: String, value: Expression, kind: VarKind },

    // Assignment to a target.
    Assign { target: AssignTarget, value: Expression },

    // Delete operation.
    Delete { target: Expression, result: Option<u32> },

    // Return statement.
    Return(Option<Expression>),

    // Throw statement.
    Throw(Expression),

    // Debugger statement.
    Debugger,

    // Comment (for debugging output).
    Comment(String),

    // Break statement with optional label.
    Break(Option<String>),

    // Continue statement with optional label.
    Continue(Option<String>),

    // Low-level goto (before structure recovery).
    Goto(BlockId),

    // Conditional goto (before structure recovery).
    CondGoto { condition: Expression, target: BlockId, fallthrough: BlockId },

    // If statement (after structure recovery).
    If { condition: Expression, then_body: Vec<Statement>, else_body: Vec<Statement> },

    // While loop (after structure recovery).
    While { condition: Expression, body: Vec<Statement> },

    // Do-while loop (after structure recovery).
    DoWhile { body: Vec<Statement>, condition: Expression },

    // For loop (after pattern detection).
    For {
        init: Option<Box<Statement>>,
        condition: Option<Expression>,
        update: Option<Box<Statement>>,
        body: Vec<Statement>,
    },

    // Switch statement (after pattern detection).
    Switch {
        discriminant: Expression,
        cases: Vec<(Expression, Vec<Statement>)>,
        default: Option<Vec<Statement>>,
    },

    // For-of loop: `for (const x of iterable) { ... }`
    ForOf {
        variable: String,
        iterable: Expression,
        body: Vec<Statement>,
    },

    // For-in loop: `for (const k in object) { ... }`
    ForIn {
        variable: String,
        object: Expression,
        body: Vec<Statement>,
    },

    // Try-catch-finally (after structure recovery).
    TryCatch {
        try_body: Vec<Statement>,
        catch_param: Option<String>,
        catch_body: Vec<Statement>,
        finally_body: Vec<Statement>,
    },

    // Block of statements.
    Block(Vec<Statement>),

    // Class declaration.
    Class {
        name: String,
        super_class: Option<Expression>,
        constructor: Option<Box<Statement>>, // Usually Statement::Let with Function expression? Or just the body?
        // Actually constructor is a function.
        // Let's store methods as (name, function_id, is_static)
        methods: Vec<ClassMethod>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassMethod {
    pub key: String,
    pub value: Expression, // Function expression
    pub body: Option<Vec<Statement>>, // Inlined body for printing
    pub is_static: bool,
    pub kind: MethodKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MethodKind {
    Constructor,
    Method,
    Getter,
    Setter,
}

// Target for assignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AssignTarget {
    // Local variable by name.
    Variable(String),

    // Register (before variable resolution).
    Register(u32),

    // Property access.
    Member { object: Expression, property: String },

    // Computed property access.
    Index { object: Expression, key: Expression },

    // Closure variable (for capturing scope).
    ClosureVar { level: u32, slot: u32 },

    // Array destructuring: `[a, b] = ...`
    DestructuringArray(Vec<Option<AssignTarget>>),

    // Array destructuring with rest: `[a, b, ...rest] = ...`
    DestructuringArrayRest {
        elements: Vec<Option<AssignTarget>>,
        rest: Box<AssignTarget>,
    },

    // Object destructuring: `{a, b: c} = ...`
    DestructuringObject(Vec<(String, AssignTarget)>),

    // Object destructuring with rest: `{a, b, ...rest} = ...`
    DestructuringObjectRest {
        properties: Vec<(String, AssignTarget)>,
        rest: Box<AssignTarget>,
    },

    // Rest element: `...x` (used inside destructuring)
    Rest(Box<AssignTarget>),
}

// Block terminator (control flow).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Terminator {
    // Unconditional jump.
    Jump(BlockId),

    // Conditional branch.
    Branch {
        condition: Expression,
        true_target: BlockId,
        false_target: BlockId,
    },

    // Return from function.
    Return(Option<Expression>),

    // Throw exception.
    Throw(Expression),

    // Switch statement.
    Switch {
        value: Expression,
        cases: Vec<(Expression, BlockId)>,
        default: BlockId,
    },

    // No terminator yet (during construction).
    None,
}

impl Statement {
    pub fn expr(e: Expression) -> Self {
        Statement::Expr(e)
    }

    pub fn let_stmt(name: impl Into<String>, value: Expression) -> Self {
        Statement::Let { name: name.into(), value, kind: VarKind::Let }
    }

    pub fn const_stmt(name: impl Into<String>, value: Expression) -> Self {
        Statement::Let { name: name.into(), value, kind: VarKind::Const }
    }

    pub fn var_stmt(name: impl Into<String>, value: Expression) -> Self {
        Statement::Let { name: name.into(), value, kind: VarKind::Var }
    }

    pub fn assign_var(name: impl Into<String>, value: Expression) -> Self {
        Statement::Assign {
            target: AssignTarget::Variable(name.into()),
            value,
        }
    }

    pub fn assign_reg(reg: u32, value: Expression) -> Self {
        Statement::Assign {
            target: AssignTarget::Register(reg),
            value,
        }
    }

    pub fn ret(value: Option<Expression>) -> Self {
        Statement::Return(value)
    }
}

impl Terminator {
    pub fn jump(target: BlockId) -> Self {
        Terminator::Jump(target)
    }

    pub fn branch(cond: Expression, true_: BlockId, false_: BlockId) -> Self {
        Terminator::Branch {
            condition: cond,
            true_target: true_,
            false_target: false_,
        }
    }

    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Jump(t) => vec![*t],
            Terminator::Branch { true_target, false_target, .. } => {
                vec![*true_target, *false_target]
            }
            Terminator::Switch { cases, default, .. } => {
                let mut targets: Vec<_> = cases.iter().map(|(_, t)| *t).collect();
                targets.push(*default);
                targets
            }
            Terminator::Return(_) | Terminator::Throw(_) | Terminator::None => vec![],
        }
    }

    pub fn is_return(&self) -> bool {
        matches!(self, Terminator::Return(_))
    }
}
