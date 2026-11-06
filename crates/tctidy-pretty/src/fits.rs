//! The fitting check: does a document render within the remaining width?
//!
//! `fits` walks a local worklist (the group's flat content followed by the
//! commands still queued after it, so trailing tokens like `;` are accounted
//! for) and returns as soon as the answer is known:
//!
//! * `true`  the instant a line break is reached (rest of the line is moot),
//! * `false` the instant the remaining width goes negative.
//!
//! Because [`crate::doc::Doc::Text`] carries its pre-measured width, this is
//! O(width) rather than O(doc size), the key to the latency budget. A
//! [`crate::doc::Doc::LineSuffix`] is zero-width (deferred content never makes a
//! group break).

use std::collections::HashMap;

use crate::doc::{Doc, GroupId, LineKind};
use crate::print::{Cmd, Mode};

/// Does `next` (followed by the already-queued `rest` commands) fit in
/// `remaining` columns? `rest` is read top-of-stack first (its last element is
/// the next command the printer would run).
pub(crate) fn fits(
    mut remaining: i32,
    next: Cmd<'_>,
    rest: &[Cmd<'_>],
    group_modes: &HashMap<GroupId, Mode>,
    must_be_flat: bool,
) -> bool {
    if remaining < 0 {
        return false;
    }
    let mut local: Vec<Cmd> = vec![next];
    let mut rest_idx = rest.len();

    loop {
        let cmd = match local.pop() {
            Some(c) => c,
            None => {
                if rest_idx == 0 {
                    return true; // consumed everything without overflowing
                }
                rest_idx -= 1;
                rest[rest_idx]
            }
        };
        let Cmd { indent, mode, doc } = cmd;
        match doc {
            Doc::Text(_, w) => {
                remaining -= *w as i32;
                if remaining < 0 {
                    return false;
                }
            }
            Doc::Concat(parts) | Doc::Fill(parts) => {
                for p in parts.iter().rev() {
                    local.push(Cmd {
                        indent,
                        mode,
                        doc: p,
                    });
                }
            }
            Doc::Line(kind) => match (mode, kind) {
                (_, LineKind::Hard | LineKind::HardLiteral) => return true,
                (Mode::Break, _) => return true,
                (Mode::Flat, LineKind::Space) => {
                    remaining -= 1;
                    if remaining < 0 {
                        return false;
                    }
                }
                (Mode::Flat, LineKind::Soft | LineKind::Trim) => {}
            },
            Doc::Group {
                content,
                should_break,
                expanded_states,
                ..
            } => {
                if must_be_flat && *should_break {
                    return false;
                }
                let m = if *should_break { Mode::Break } else { mode };
                let c = match expanded_states {
                    Some(states) => states.first().unwrap_or(content),
                    None => content,
                };
                local.push(Cmd {
                    indent,
                    mode: m,
                    doc: c,
                });
            }
            Doc::Indent(c)
            | Doc::Align { content: c, .. }
            | Doc::AlignCurrent(c)
            | Doc::Label(_, c) => {
                local.push(Cmd {
                    indent,
                    mode,
                    doc: c,
                });
            }
            Doc::IndentIfBreak { content, .. } => {
                local.push(Cmd {
                    indent,
                    mode,
                    doc: content,
                });
            }
            Doc::IfBreak {
                broke,
                flat,
                group_id,
            } => {
                let m = match group_id {
                    Some(g) => group_modes.get(g).copied().unwrap_or(mode),
                    None => mode,
                };
                let chosen = if m == Mode::Break { broke } else { flat };
                local.push(Cmd {
                    indent,
                    mode,
                    doc: chosen,
                });
            }
            Doc::LineSuffix(_) | Doc::LineSuffixBoundary | Doc::BreakParent => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builders::{concat, group, hard_line, text};
    use crate::print::{propagate_breaks, Indent};

    /// A group carrying a forced break can never satisfy `must_be_flat`, while
    /// the same query without the flat requirement short-circuits to true at
    /// the hard line. Both `print` call sites pass `must_be_flat = false`, so
    /// this branch is only reachable directly.
    #[test]
    fn flat_required_group_with_forced_break_does_not_fit() {
        let mut doc = group(concat([text("a"), hard_line(), text("b")]));
        propagate_breaks(&mut doc);
        let cmd = Cmd {
            indent: Indent::root(),
            mode: Mode::Flat,
            doc: &doc,
        };
        assert!(!fits(80, cmd, &[], &HashMap::new(), true));
        assert!(fits(80, cmd, &[], &HashMap::new(), false));
    }
}
