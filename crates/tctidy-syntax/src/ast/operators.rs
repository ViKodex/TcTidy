//! Expression wrappers and operator classification.
//!
//! `Expr` is the sum type over every expression kind (the rust-analyzer pattern
//! for "any expression"). Postfix chains (`.`/`[]`/`^`/`()`) are left-associative
//! nodes built by the parser; their accessors expose the base and the suffix.

use super::{ast_node, child, children, AstNode};
use crate::kind::SyntaxKind::{self, *};
use crate::syntax_node::{SyntaxNode, SyntaxToken};

ast_node!(LiteralExpr, LITERAL_EXPR);
ast_node!(NameRef, NAME_REF);
ast_node!(ParenExpr, PAREN_EXPR);
ast_node!(BinExpr, BIN_EXPR);
ast_node!(UnaryExpr, UNARY_EXPR);
ast_node!(QualifiedRef, QUALIFIED_REF);
ast_node!(IndexExpr, INDEX_EXPR);
ast_node!(DerefExpr, DEREF_EXPR);
ast_node!(CallExpr, CALL_EXPR);
ast_node!(ArgList, ARG_LIST);
ast_node!(Arg, ARG);
ast_node!(NamedArg, NAMED_ARG);

impl BinExpr {
    pub fn lhs(&self) -> Option<Expr> {
        children(self.syntax()).next()
    }
    pub fn rhs(&self) -> Option<Expr> {
        children(self.syntax()).nth(1)
    }
    /// The operator token (the first non-trivia token that is a binary operator).
    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| BinOp::from_kind(t.kind()).is_some())
    }
    pub fn op(&self) -> Option<BinOp> {
        self.op_token().and_then(|t| BinOp::from_kind(t.kind()))
    }
}

impl UnaryExpr {
    pub fn operand(&self) -> Option<Expr> {
        child(self.syntax())
    }
    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| matches!(t.kind(), NOT_KW | MINUS | PLUS))
    }
}

impl QualifiedRef {
    /// The receiver expression to the left of the `.`.
    pub fn base(&self) -> Option<Expr> {
        children(self.syntax()).next()
    }
    /// The member name to the right of the `.` (the trailing `NAME_REF`; in a
    /// flat `a.b` both children are `NAME_REF`, so take the last).
    pub fn segment(&self) -> Option<NameRef> {
        children(self.syntax()).last()
    }
}

impl IndexExpr {
    pub fn base(&self) -> Option<Expr> {
        children(self.syntax()).next()
    }
    /// The index expressions inside `[...]` (the base is the first child).
    pub fn indices(&self) -> impl Iterator<Item = Expr> {
        children(self.syntax()).skip(1)
    }
}

impl DerefExpr {
    pub fn base(&self) -> Option<Expr> {
        child(self.syntax())
    }
}

impl CallExpr {
    pub fn callee(&self) -> Option<Expr> {
        children(self.syntax()).next()
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        child(self.syntax())
    }
}

impl ArgList {
    pub fn args(&self) -> impl Iterator<Item = CallArg> {
        children(self.syntax())
    }
}

impl Arg {
    pub fn expr(&self) -> Option<Expr> {
        child(self.syntax())
    }
}

impl NamedArg {
    /// The parameter name to the left of `:=` / `=>`.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| matches!(t.kind(), IDENT | TWINCAT_IDENT))
    }
    pub fn assign_token(&self) -> Option<SyntaxToken> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| matches!(t.kind(), ASSIGN | ARROW))
    }
    pub fn value(&self) -> Option<Expr> {
        child(self.syntax())
    }
}

/// Binary operator, classified from the operator token's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    And,
    Or,
    Xor,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    /// Assignment used in expression position (non-standard, but appears in real
    /// code as a condition, e.g. `IF x := 1 THEN`). Kept so such input round-trips.
    Assign,
}

impl BinOp {
    pub fn from_kind(kind: SyntaxKind) -> Option<BinOp> {
        Some(match kind {
            PLUS => BinOp::Add,
            MINUS => BinOp::Sub,
            STAR => BinOp::Mul,
            SLASH => BinOp::Div,
            MOD_KW => BinOp::Mod,
            POW => BinOp::Pow,
            AND_KW | AND_THEN_KW | AMP => BinOp::And,
            OR_KW | OR_ELSE_KW => BinOp::Or,
            XOR_KW => BinOp::Xor,
            EQ => BinOp::Eq,
            NEQ => BinOp::Neq,
            LT => BinOp::Lt,
            GT => BinOp::Gt,
            LTE => BinOp::Lte,
            GTE => BinOp::Gte,
            ASSIGN => BinOp::Assign,
            _ => return None,
        })
    }
}

/// Any expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    Literal(LiteralExpr),
    Name(NameRef),
    Paren(ParenExpr),
    Bin(BinExpr),
    Unary(UnaryExpr),
    Qualified(QualifiedRef),
    Index(IndexExpr),
    Deref(DerefExpr),
    Call(CallExpr),
}

impl AstNode for Expr {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            LITERAL_EXPR
                | NAME_REF
                | PAREN_EXPR
                | BIN_EXPR
                | UNARY_EXPR
                | QUALIFIED_REF
                | INDEX_EXPR
                | DEREF_EXPR
                | CALL_EXPR
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        let expr = match node.kind() {
            LITERAL_EXPR => Expr::Literal(LiteralExpr::cast(node)?),
            NAME_REF => Expr::Name(NameRef::cast(node)?),
            PAREN_EXPR => Expr::Paren(ParenExpr::cast(node)?),
            BIN_EXPR => Expr::Bin(BinExpr::cast(node)?),
            UNARY_EXPR => Expr::Unary(UnaryExpr::cast(node)?),
            QUALIFIED_REF => Expr::Qualified(QualifiedRef::cast(node)?),
            INDEX_EXPR => Expr::Index(IndexExpr::cast(node)?),
            DEREF_EXPR => Expr::Deref(DerefExpr::cast(node)?),
            CALL_EXPR => Expr::Call(CallExpr::cast(node)?),
            _ => return None,
        };
        Some(expr)
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Expr::Literal(n) => n.syntax(),
            Expr::Name(n) => n.syntax(),
            Expr::Paren(n) => n.syntax(),
            Expr::Bin(n) => n.syntax(),
            Expr::Unary(n) => n.syntax(),
            Expr::Qualified(n) => n.syntax(),
            Expr::Index(n) => n.syntax(),
            Expr::Deref(n) => n.syntax(),
            Expr::Call(n) => n.syntax(),
        }
    }
}

/// One call argument: positional (`Arg`) or named (`name := expr` / `name => expr`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallArg {
    Positional(Arg),
    Named(NamedArg),
}

impl AstNode for CallArg {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind, ARG | NAMED_ARG)
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        let arg = match node.kind() {
            ARG => CallArg::Positional(Arg::cast(node)?),
            NAMED_ARG => CallArg::Named(NamedArg::cast(node)?),
            _ => return None,
        };
        Some(arg)
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            CallArg::Positional(n) => n.syntax(),
            CallArg::Named(n) => n.syntax(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::TcStLang;
    use rowan::{GreenNodeBuilder, Language};

    /// A BIN_EXPR that lost its operator (only reachable on a malformed tree):
    /// the accessors must return `None`, never panic.
    #[test]
    fn bin_expr_without_operator_token_yields_none() {
        let mut builder = GreenNodeBuilder::new();
        builder.start_node(TcStLang::kind_to_raw(BIN_EXPR));
        builder.token(TcStLang::kind_to_raw(IDENT), "a");
        builder.finish_node();
        let root = SyntaxNode::new_root(builder.finish());

        let bin = BinExpr::cast(root).expect("node is a BIN_EXPR");
        assert!(bin.op_token().is_none());
        assert!(bin.op().is_none());
    }
}
