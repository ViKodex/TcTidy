//! The fitting check: does a document render within the remaining width?
//!
//! `fits` walks a local worklist (the group's flat content followed by the
//! commands still queued after it, so trailing tokens like `;` are accounted
//! for) and returns as soon as the answer is known:
//!
//! * `true`  the instant a line break is reached (rest of the line is moot),
//! * `false` the instant the remaining width goes negative,
//! * `false` the instant flat content holds a forced break (a group that must
//!   break, a hard line, a `BreakParent`): it has no flat rendering.
//!
//! The last rule is what keeps a group from printing flat around a forced
//! break that sits behind a [`crate::doc::Doc::IfBreak`] or a
//! [`crate::doc::Doc::IndentIfBreak`]: break propagation stops there, and the
//! printer never breaks a group inside a flat one.
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
                (Mode::Flat, LineKind::Hard | LineKind::HardLiteral) => return false,
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
                if *should_break && (must_be_flat || mode == Mode::Flat) {
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
            Doc::BreakParent => {
                if mode == Mode::Flat {
                    return false;
                }
            }
            Doc::LineSuffix(_) | Doc::LineSuffixBoundary => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builders::{concat, group, hard_line, text};
    use crate::print::{propagate_breaks, Indent};

    /// A group carrying a forced break never fits as flat content. In break
    /// mode the walk short-circuits to true at its hard line, unless the
    /// caller requires a flat rendering. Both `print` call sites pass
    /// `must_be_flat = false`, so that last branch is only reachable directly.
    #[test]
    fn group_with_forced_break_does_not_fit_as_flat_content() {
        let mut doc = group(concat([text("a"), hard_line(), text("b")]));
        propagate_breaks(&mut doc);
        let cmd = |mode| Cmd {
            indent: Indent::root(),
            mode,
            doc: &doc,
        };
        assert!(!fits(80, cmd(Mode::Flat), &[], &HashMap::new(), false));
        assert!(!fits(80, cmd(Mode::Flat), &[], &HashMap::new(), true));
        assert!(fits(80, cmd(Mode::Break), &[], &HashMap::new(), false));
        assert!(!fits(80, cmd(Mode::Break), &[], &HashMap::new(), true));
    }

    /// A forced break in the commands queued after the measured content ends
    /// the line: the queue is in break mode, so what follows is moot.
    #[test]
    fn forced_break_in_the_queued_commands_ends_the_line() {
        let mut queued = group(concat([text("b"), hard_line(), text("c")]));
        propagate_breaks(&mut queued);
        let next = text("a");
        let cmd = Cmd {
            indent: Indent::root(),
            mode: Mode::Flat,
            doc: &next,
        };
        let rest = [Cmd {
            indent: Indent::root(),
            mode: Mode::Break,
            doc: &queued,
        }];
        assert!(fits(2, cmd, &rest, &HashMap::new(), false));
        assert!(!fits(1, cmd, &rest, &HashMap::new(), false));
    }
}
