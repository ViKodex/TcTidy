//! Render a [`Doc`] as an S-expression for tests and inspection.
//!
//! This is a debugging aid only; it is not part of the formatting output path,
//! so it is left out of the coverage metric (see the coverage command in
//! MAINTAINING.md).

use crate::doc::Doc;

/// Produce a stable, human-readable S-expression for `doc`.
pub fn to_sexpr(doc: &Doc) -> String {
    let mut out = String::new();
    write_doc(doc, &mut out);
    out
}

fn write_doc(doc: &Doc, out: &mut String) {
    match doc {
        Doc::Text(s, _) => {
            out.push('"');
            out.push_str(s);
            out.push('"');
        }
        Doc::Concat(parts) => {
            out.push_str("(concat");
            for p in parts {
                out.push(' ');
                write_doc(p, out);
            }
            out.push(')');
        }
        Doc::Line(kind) => {
            out.push_str(&format!("(line {kind:?})"));
        }
        Doc::Group {
            content,
            should_break,
            ..
        } => {
            out.push_str(if *should_break { "(group! " } else { "(group " });
            write_doc(content, out);
            out.push(')');
        }
        Doc::Indent(content) => {
            out.push_str("(indent ");
            write_doc(content, out);
            out.push(')');
        }
        Doc::Align { content, .. } => {
            out.push_str("(align ");
            write_doc(content, out);
            out.push(')');
        }
        Doc::AlignCurrent(content) => {
            out.push_str("(align-current ");
            write_doc(content, out);
            out.push(')');
        }
        Doc::IfBreak { broke, flat, .. } => {
            out.push_str("(if-break ");
            write_doc(broke, out);
            out.push(' ');
            write_doc(flat, out);
            out.push(')');
        }
        Doc::IndentIfBreak { content, .. } => {
            out.push_str("(indent-if-break ");
            write_doc(content, out);
            out.push(')');
        }
        Doc::LineSuffix(content) => {
            out.push_str("(line-suffix ");
            write_doc(content, out);
            out.push(')');
        }
        Doc::LineSuffixBoundary => out.push_str("(line-suffix-boundary)"),
        Doc::BreakParent => out.push_str("(break-parent)"),
        Doc::Fill(items) => {
            out.push_str("(fill");
            for i in items {
                out.push(' ');
                write_doc(i, out);
            }
            out.push(')');
        }
        Doc::Label(name, content) => {
            out.push_str(&format!("(label {name:?} "));
            write_doc(content, out);
            out.push(')');
        }
    }
}
