//! Typed node wrappers for the declaration grammar (POU headers, `TYPE`
//! definitions, `VAR` blocks), the `Type` sum type over every type-reference
//! kind, and the statement grammar.

use super::operators::Expr;
use super::{ast_node, child, children, token, AstNode};
use crate::kind::SyntaxKind::{self, *};
use crate::syntax_node::{SyntaxNode, SyntaxToken};

/// First direct child token that begins a name (`IDENT`/`TWINCAT_IDENT`/`THIS`/`SUPER`).
fn name_token(parent: &SyntaxNode) -> Option<SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| matches!(t.kind(), IDENT | TWINCAT_IDENT | THIS_KW | SUPER_KW))
}

// ---- roots ----

ast_node!(SourceFile, SOURCE_FILE);
ast_node!(DeclPart, DECL_PART);
ast_node!(ImplPart, IMPL_PART);
ast_node!(StmtList, STMT_LIST);

// ---- POU headers ----

ast_node!(ProgramDecl, PROGRAM_DECL);
ast_node!(FunctionDecl, FUNCTION_DECL);
ast_node!(FunctionBlockDecl, FUNCTION_BLOCK_DECL);
ast_node!(MethodDecl, METHOD_DECL);
ast_node!(PropertyDecl, PROPERTY_DECL);
ast_node!(InterfaceDecl, INTERFACE_DECL);
ast_node!(ActionDecl, ACTION_DECL);

ast_node!(ExtendsClause, EXTENDS_CLAUSE);
impl ExtendsClause {
    pub fn extends_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::EXTENDS_KW)
    }
    /// The first name of the (dotted) base-type reference.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| matches!(token.kind(), IDENT | TWINCAT_IDENT | THIS_KW | SUPER_KW))
    }
}

ast_node!(ImplementsClause, IMPLEMENTS_CLAUSE);
impl ImplementsClause {
    pub fn implements_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::IMPLEMENTS_KW)
    }
}

ast_node!(ReturnType, RETURN_TYPE);
impl ReturnType {
    pub fn colon(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::COLON)
    }
    pub fn ty(&self) -> Option<Type> {
        child(self.syntax())
    }
}

// ---- VAR blocks ----

ast_node!(VarBlock, VAR_BLOCK);
impl VarBlock {
    pub fn decls(&self) -> impl Iterator<Item = VarDecl> {
        children(self.syntax())
    }
}

ast_node!(VarDecl, VAR_DECL);
impl VarDecl {
    pub fn name_list(&self) -> Option<VarNameList> {
        child(self.syntax())
    }
    pub fn at_clause(&self) -> Option<AtClause> {
        child(self.syntax())
    }
    pub fn colon(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::COLON)
    }
    pub fn ty(&self) -> Option<Type> {
        child(self.syntax())
    }
    pub fn init(&self) -> Option<InitExpr> {
        child(self.syntax())
    }
    pub fn semicolon(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::SEMICOLON)
    }
}

ast_node!(VarNameList, VAR_NAME_LIST);
impl VarNameList {
    pub fn names(&self) -> impl Iterator<Item = SyntaxToken> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| matches!(t.kind(), IDENT | TWINCAT_IDENT | THIS_KW | SUPER_KW))
    }
}

ast_node!(AtClause, AT_CLAUSE);
impl AtClause {
    pub fn at_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::AT_KW)
    }
    pub fn address(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::DIRECT_ADDRESS)
    }
}

ast_node!(InitExpr, INIT_EXPR);
impl InitExpr {
    pub fn assign_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::ASSIGN)
    }
    /// The initializer value(s); more than one for an array-of-struct
    /// initializer `(..), (..), (..)`.
    pub fn exprs(&self) -> impl Iterator<Item = Expr> {
        children(self.syntax())
    }
    /// The single initializer value (the common case).
    pub fn expr(&self) -> Option<Expr> {
        child(self.syntax())
    }
}

// ---- TYPE definitions ----

ast_node!(TypeDecl, TYPE_DECL);
impl TypeDecl {
    pub fn type_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::TYPE_KW)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(self.syntax())
    }
    pub fn extends(&self) -> Option<ExtendsClause> {
        child(self.syntax())
    }
    pub fn colon(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::COLON)
    }
    /// The right-hand-side type definition (struct / enum / alias / …).
    pub fn type_def(&self) -> Option<Type> {
        child(self.syntax())
    }
    pub fn init(&self) -> Option<InitExpr> {
        child(self.syntax())
    }
}

ast_node!(StructType, STRUCT_TYPE);
impl StructType {
    pub fn members(&self) -> impl Iterator<Item = VarDecl> {
        children(self.syntax())
    }
}

ast_node!(UnionType, UNION_TYPE);
impl UnionType {
    pub fn members(&self) -> impl Iterator<Item = VarDecl> {
        children(self.syntax())
    }
}

ast_node!(EnumType, ENUM_TYPE);
impl EnumType {
    pub fn variants(&self) -> impl Iterator<Item = EnumVariant> {
        children(self.syntax())
    }
    /// Optional base type after the variant list, e.g. `(...) UINT`.
    pub fn base(&self) -> Option<Type> {
        child(self.syntax())
    }
}

ast_node!(EnumVariant, ENUM_VARIANT);
impl EnumVariant {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(self.syntax())
    }
    pub fn init(&self) -> Option<InitExpr> {
        child(self.syntax())
    }
}

ast_node!(SubrangeType, SUBRANGE_TYPE);
impl SubrangeType {
    pub fn lo(&self) -> Option<Expr> {
        children(self.syntax()).next()
    }
    pub fn hi(&self) -> Option<Expr> {
        children(self.syntax()).nth(1)
    }
}

ast_node!(ArrayType, ARRAY_TYPE);
impl ArrayType {
    pub fn dims(&self) -> Option<ArrayDims> {
        child(self.syntax())
    }
    /// The element type after `OF`.
    pub fn elem(&self) -> Option<Type> {
        child(self.syntax())
    }
}

ast_node!(ArrayDims, ARRAY_DIMS);
impl ArrayDims {
    pub fn dims(&self) -> impl Iterator<Item = ArrayDim> {
        children(self.syntax())
    }
}

ast_node!(ArrayDim, ARRAY_DIM);
impl ArrayDim {
    pub fn lo(&self) -> Option<Expr> {
        children(self.syntax()).next()
    }
    pub fn hi(&self) -> Option<Expr> {
        children(self.syntax()).nth(1)
    }
}

ast_node!(PointerType, POINTER_TYPE);
impl PointerType {
    pub fn inner(&self) -> Option<Type> {
        child(self.syntax())
    }
}

ast_node!(ReferenceType, REFERENCE_TYPE);
impl ReferenceType {
    pub fn inner(&self) -> Option<Type> {
        child(self.syntax())
    }
}

ast_node!(StringType, STRING_TYPE);
impl StringType {
    /// The optional length expression inside `STRING(n)` / `STRING[n]`.
    pub fn size(&self) -> Option<Expr> {
        child(self.syntax())
    }
}

ast_node!(TypeRef, TYPE_REF);
impl TypeRef {
    pub fn generic_args(&self) -> Option<GenericArgs> {
        child(self.syntax())
    }
}

ast_node!(GenericArgs, GENERIC_ARGS);
impl GenericArgs {
    pub fn value(&self) -> Option<Expr> {
        child(self.syntax())
    }
}

/// Any type reference (the right-hand side of a `:` in a declaration).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Ref(TypeRef),
    Struct(StructType),
    Union(UnionType),
    Enum(EnumType),
    Subrange(SubrangeType),
    Array(ArrayType),
    Pointer(PointerType),
    Reference(ReferenceType),
    String(StringType),
}

impl AstNode for Type {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            TYPE_REF
                | STRUCT_TYPE
                | UNION_TYPE
                | ENUM_TYPE
                | SUBRANGE_TYPE
                | ARRAY_TYPE
                | POINTER_TYPE
                | REFERENCE_TYPE
                | STRING_TYPE
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        let ty = match node.kind() {
            TYPE_REF => Type::Ref(TypeRef::cast(node)?),
            STRUCT_TYPE => Type::Struct(StructType::cast(node)?),
            UNION_TYPE => Type::Union(UnionType::cast(node)?),
            ENUM_TYPE => Type::Enum(EnumType::cast(node)?),
            SUBRANGE_TYPE => Type::Subrange(SubrangeType::cast(node)?),
            ARRAY_TYPE => Type::Array(ArrayType::cast(node)?),
            POINTER_TYPE => Type::Pointer(PointerType::cast(node)?),
            REFERENCE_TYPE => Type::Reference(ReferenceType::cast(node)?),
            STRING_TYPE => Type::String(StringType::cast(node)?),
            _ => return None,
        };
        Some(ty)
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Type::Ref(n) => n.syntax(),
            Type::Struct(n) => n.syntax(),
            Type::Union(n) => n.syntax(),
            Type::Enum(n) => n.syntax(),
            Type::Subrange(n) => n.syntax(),
            Type::Array(n) => n.syntax(),
            Type::Pointer(n) => n.syntax(),
            Type::Reference(n) => n.syntax(),
            Type::String(n) => n.syntax(),
        }
    }
}

// ---- statements ----

ast_node!(IfStmt, IF_STMT);
impl IfStmt {
    pub fn condition(&self) -> Option<Expr> {
        child(self.syntax())
    }
    pub fn then_branch(&self) -> Option<StmtList> {
        child(self.syntax())
    }
    pub fn if_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::IF_KW)
    }
    pub fn then_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::THEN_KW)
    }
    pub fn end_if_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::END_IF_KW)
    }
}

ast_node!(TryStmt, TRY_STMT);
impl TryStmt {
    pub fn body(&self) -> Option<StmtList> {
        child(self.syntax())
    }
    pub fn catch_clause(&self) -> Option<CatchClause> {
        child(self.syntax())
    }
    pub fn finally_clause(&self) -> Option<FinallyClause> {
        child(self.syntax())
    }
    pub fn try_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::TRY_KW)
    }
    pub fn endtry_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), SyntaxKind::ENDTRY_KW)
    }
}

ast_node!(CatchClause, CATCH_CLAUSE);
impl CatchClause {
    pub fn exception_variable(&self) -> Option<SyntaxToken> {
        name_token(self.syntax())
    }
    pub fn body(&self) -> Option<StmtList> {
        child(self.syntax())
    }
}

ast_node!(FinallyClause, FINALLY_CLAUSE);
impl FinallyClause {
    pub fn body(&self) -> Option<StmtList> {
        child(self.syntax())
    }
}
