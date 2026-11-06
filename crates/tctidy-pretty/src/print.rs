//! The printing engine: turns a [`Doc`] into a `String` for a given width.
//!
//! A command-stack algorithm, iterative so deeply nested documents
//! cannot overflow the stack: each command pairs a sub-doc with the indentation
//! and [`Mode`] it should render in. Groups choose their mode by asking
//! [`crate::fits::fits`] whether their flat rendering fits the remaining width.

use std::collections::HashMap;

use crate::doc::{Doc, GroupId, IndentSpec, LineKind};
use crate::options::{IndentStyle, PrintOptions};

/// Whether a document is currently being laid out on a single line (`Flat`) or
/// with its breaks taken (`Break`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Flat,
    Break,
}

/// Indentation state: a column count, materialized into spaces/tabs lazily.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Indent {
    pub(crate) length: u16,
}

impl Indent {
    pub(crate) fn root() -> Self {
        Indent { length: 0 }
    }
    fn level(self, opts: &PrintOptions) -> Self {
        Indent {
            length: self.length.saturating_add(opts.indent_width),
        }
    }
    fn align(self, spec: IndentSpec, opts: &PrintOptions) -> Self {
        match spec {
            IndentSpec::Levels(n) => {
                let delta = n as i32 * opts.indent_width as i32;
                let v = (self.length as i32 + delta).clamp(0, u16::MAX as i32);
                Indent { length: v as u16 }
            }
            IndentSpec::Spaces(n) => Indent {
                length: self.length.saturating_add(n),
            },
        }
    }
}

/// One unit of work on the print stack.
#[derive(Clone, Copy)]
pub(crate) struct Cmd<'a> {
    pub(crate) indent: Indent,
    pub(crate) mode: Mode,
    pub(crate) doc: &'a Doc,
}

/// Render `doc` to a string under `opts`.
pub fn print(doc: &Doc, opts: &PrintOptions) -> String {
    let mut doc = doc.clone();
    propagate_breaks(&mut doc);

    let mut out = String::new();
    let mut pos: u16 = 0;
    let mut cmds: Vec<Cmd> = vec![Cmd {
        indent: Indent::root(),
        mode: Mode::Break,
        doc: &doc,
    }];
    let mut suffixes: Vec<Cmd> = Vec::new();
    let mut group_modes: HashMap<GroupId, Mode> = HashMap::new();
    // Where each line's suffixes were flushed, for `align_line_suffixes`.
    let mut suffix_starts: Vec<SuffixStart> = Vec::new();

    while !cmds.is_empty() || !suffixes.is_empty() {
        // Reuse the normal command stack for suffixes left at EOF. Deferred
        // content can contain every Doc shape, including literal line breaks;
        // a reduced tail renderer would silently discard those documents.
        if cmds.is_empty() {
            if opts.align_line_suffixes {
                record_suffix_start(&out, pos, &mut suffix_starts);
            }
            let drained: Vec<Cmd> = suffixes.split_off(0);
            for c in drained.into_iter().rev() {
                cmds.push(c);
            }
        }

        let cmd = cmds.pop().expect("render queue is not empty");
        let Cmd {
            indent,
            mode,
            doc: d,
        } = cmd;
        match d {
            Doc::Text(s, w) => {
                out.push_str(s);
                pos = pos.saturating_add(*w);
            }
            Doc::Concat(parts) => {
                for p in parts.iter().rev() {
                    cmds.push(Cmd {
                        indent,
                        mode,
                        doc: p,
                    });
                }
            }
            Doc::Line(kind) => {
                let is_hard = matches!(kind, LineKind::Hard | LineKind::HardLiteral);
                if mode == Mode::Flat && !is_hard {
                    match kind {
                        LineKind::Space => {
                            out.push(' ');
                            pos = pos.saturating_add(1);
                        }
                        LineKind::Trim => {
                            trim_trailing_ws(&mut out);
                            pos = current_line_width(&out);
                        }
                        _ => {} // Soft: nothing
                    }
                } else {
                    // Break (or a hard line in any mode).
                    if !suffixes.is_empty() && !matches!(kind, LineKind::Trim) {
                        // Flush deferred line-suffix content before the break:
                        // re-push this line, then the suffixes (in order).
                        if opts.align_line_suffixes {
                            record_suffix_start(&out, pos, &mut suffix_starts);
                        }
                        cmds.push(Cmd {
                            indent,
                            mode,
                            doc: d,
                        });
                        let drained: Vec<Cmd> = suffixes.split_off(0);
                        for c in drained.into_iter().rev() {
                            cmds.push(c);
                        }
                        continue;
                    }
                    trim_trailing_ws(&mut out);
                    if matches!(kind, LineKind::Trim) {
                        pos = current_line_width(&out);
                    } else {
                        out.push_str(opts.eol.as_str());
                        if matches!(kind, LineKind::HardLiteral) {
                            pos = 0;
                        } else {
                            write_indent(&mut out, indent, opts);
                            pos = indent.length;
                        }
                    }
                }
            }
            Doc::Group {
                id,
                content,
                should_break,
                expanded_states,
            } => {
                if let Some(states) = expanded_states {
                    print_conditional_group(
                        states,
                        *should_break,
                        indent,
                        mode,
                        pos,
                        opts,
                        &mut cmds,
                        &group_modes,
                    );
                } else {
                    let m = if mode == Mode::Flat {
                        Mode::Flat
                    } else if *should_break {
                        Mode::Break
                    } else if crate::fits::fits(
                        opts.width as i32 - pos as i32,
                        Cmd {
                            indent,
                            mode: Mode::Flat,
                            doc: content,
                        },
                        &cmds,
                        &group_modes,
                        false,
                    ) {
                        Mode::Flat
                    } else {
                        Mode::Break
                    };
                    if let Some(gid) = id {
                        group_modes.insert(*gid, m);
                    }
                    cmds.push(Cmd {
                        indent,
                        mode: m,
                        doc: content,
                    });
                }
            }
            Doc::Indent(c) => cmds.push(Cmd {
                indent: indent.level(opts),
                mode,
                doc: c,
            }),
            Doc::Align { spec, content } => cmds.push(Cmd {
                indent: indent.align(*spec, opts),
                mode,
                doc: content,
            }),
            Doc::AlignCurrent(content) => cmds.push(Cmd {
                // Subsequent line breaks indent to the current column.
                indent: Indent { length: pos },
                mode,
                doc: content,
            }),
            Doc::IfBreak {
                broke,
                flat,
                group_id,
            } => {
                let m = resolve_mode(*group_id, mode, &group_modes);
                let chosen = if m == Mode::Break { broke } else { flat };
                cmds.push(Cmd {
                    indent,
                    mode,
                    doc: chosen,
                });
            }
            Doc::IndentIfBreak { content, group_id } => {
                let m = group_modes.get(group_id).copied().unwrap_or(mode);
                let i = if m == Mode::Break {
                    indent.level(opts)
                } else {
                    indent
                };
                cmds.push(Cmd {
                    indent: i,
                    mode,
                    doc: content,
                });
            }
            Doc::LineSuffix(c) => suffixes.push(Cmd {
                indent,
                mode,
                doc: c,
            }),
            Doc::LineSuffixBoundary => {
                if !suffixes.is_empty() {
                    let drained: Vec<Cmd> = suffixes.split_off(0);
                    for c in drained.into_iter().rev() {
                        cmds.push(c);
                    }
                }
            }
            Doc::BreakParent => {}
            Doc::Fill(parts) => {
                // Packing-fill is not emitted by the ST rules yet; degrade to the
                // current mode (one-per-line when broken). Documented in fits.rs.
                for p in parts.iter().rev() {
                    cmds.push(Cmd {
                        indent,
                        mode,
                        doc: p,
                    });
                }
            }
            Doc::Label(_, c) => cmds.push(Cmd {
                indent,
                mode,
                doc: c,
            }),
        }
    }

    align_suffixes(&mut out, &suffix_starts);
    out
}

/// The byte offset and column at which a line's suffixes start.
struct SuffixStart {
    offset: usize,
    column: u16,
}

/// Note where the suffixes of the current line start, unless the line holds
/// nothing but indentation (there is no content to align against).
fn record_suffix_start(out: &str, pos: u16, starts: &mut Vec<SuffixStart>) {
    let line_start = out.rfind('\n').map_or(0, |i| i + 1);
    if out[line_start..].bytes().any(|b| b != b' ' && b != b'\t') {
        starts.push(SuffixStart {
            offset: out.len(),
            column: pos,
        });
    }
}

/// Pad the suffixes of every run of consecutive lines to the column of the
/// run's rightmost suffix. `starts` is in output order, at most one per line.
fn align_suffixes(out: &mut String, starts: &[SuffixStart]) {
    if starts.len() < 2 {
        return;
    }
    let mut lines = Vec::with_capacity(starts.len());
    let (mut line, mut scanned) = (0usize, 0usize);
    for start in starts {
        line += out[scanned..start.offset].matches('\n').count();
        scanned = start.offset;
        lines.push(line);
    }
    let mut pads = vec![0u16; starts.len()];
    let mut first = 0;
    while first < starts.len() {
        let mut end = first + 1;
        while end < starts.len() && lines[end] == lines[end - 1] + 1 {
            end += 1;
        }
        let column = starts[first..end]
            .iter()
            .map(|s| s.column)
            .max()
            .unwrap_or(0);
        for (pad, start) in pads[first..end].iter_mut().zip(&starts[first..end]) {
            *pad = column - start.column;
        }
        first = end;
    }
    for (start, pad) in starts.iter().zip(pads).rev() {
        if pad > 0 {
            out.insert_str(start.offset, &" ".repeat(usize::from(pad)));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn print_conditional_group<'a>(
    states: &'a [Doc],
    should_break: bool,
    indent: Indent,
    mode: Mode,
    pos: u16,
    opts: &PrintOptions,
    cmds: &mut Vec<Cmd<'a>>,
    group_modes: &HashMap<GroupId, Mode>,
) {
    let first = states.first();
    if mode == Mode::Flat && !should_break {
        if let Some(s) = first {
            cmds.push(Cmd {
                indent,
                mode: Mode::Flat,
                doc: s,
            });
        }
        return;
    }
    // Try each state (most-flat first) in break mode; first that fits wins.
    for s in states {
        if crate::fits::fits(
            opts.width as i32 - pos as i32,
            Cmd {
                indent,
                mode: Mode::Break,
                doc: s,
            },
            cmds,
            group_modes,
            false,
        ) {
            cmds.push(Cmd {
                indent,
                mode: Mode::Break,
                doc: s,
            });
            return;
        }
    }
    if let Some(s) = states.last() {
        cmds.push(Cmd {
            indent,
            mode: Mode::Break,
            doc: s,
        });
    }
}

fn resolve_mode(group_id: Option<GroupId>, fallback: Mode, modes: &HashMap<GroupId, Mode>) -> Mode {
    match group_id {
        Some(g) => modes.get(&g).copied().unwrap_or(fallback),
        None => fallback,
    }
}

fn write_indent(out: &mut String, indent: Indent, opts: &PrintOptions) {
    match opts.indent_style {
        IndentStyle::Spaces => {
            for _ in 0..indent.length {
                out.push(' ');
            }
        }
        IndentStyle::Tabs => {
            let unit = opts.indent_width.max(1);
            for _ in 0..(indent.length / unit) {
                out.push('\t');
            }
            for _ in 0..(indent.length % unit) {
                out.push(' ');
            }
        }
    }
}

/// Strip indentation from the current (last) line of `out` **only when that line
/// is otherwise blank**. On a line that has content, trailing spaces/tabs are
/// meaningful (verbatim comment text) and are preserved for byte-exact output.
fn trim_trailing_ws(out: &mut String) {
    let line_start = out.rfind('\n').map(|i| i + 1).unwrap_or(0);
    if out[line_start..].bytes().all(|b| b == b' ' || b == b'\t') {
        out.truncate(line_start);
    }
}

/// Display width of the current (last) line of `out`.
fn current_line_width(out: &str) -> u16 {
    let start = out.rfind('\n').map(|i| i + 1).unwrap_or(0);
    out[start..].chars().count().min(u16::MAX as usize) as u16
}

/// Bottom-up pass that sets `should_break` on every group transitively
/// containing a `BreakParent` or a hard line. Runs once before printing so the
/// engine's group decisions are local. Returns whether `doc` forces a break in
/// its parent.
///
/// Recursive: depth is bounded by document nesting (the parser caps construct
/// nesting via its fuel budget, so the printer never sees pathological depth).
pub(crate) fn propagate_breaks(doc: &mut Doc) -> bool {
    match doc {
        Doc::Text(_, _) => false,
        Doc::Line(LineKind::Hard | LineKind::HardLiteral) => true,
        Doc::Line(_) => false,
        Doc::BreakParent => true,
        Doc::Concat(parts) | Doc::Fill(parts) => {
            let mut forced = false;
            for c in parts.iter_mut() {
                if propagate_breaks(c) {
                    forced = true;
                }
            }
            forced
        }
        Doc::Indent(c) | Doc::Align { content: c, .. } | Doc::Label(_, c) => propagate_breaks(c),
        Doc::AlignCurrent(c) => propagate_breaks(c),
        Doc::LineSuffix(c) => {
            // A break inside deferred content does not force the current group.
            propagate_breaks(c);
            false
        }
        Doc::IfBreak { broke, flat, .. } => {
            propagate_breaks(broke);
            propagate_breaks(flat);
            false
        }
        Doc::IndentIfBreak { content, .. } => {
            propagate_breaks(content);
            false
        }
        Doc::Group {
            content,
            should_break,
            expanded_states,
            ..
        } => {
            let child = propagate_breaks(content);
            if let Some(states) = expanded_states {
                for s in states.iter_mut() {
                    propagate_breaks(s);
                }
            }
            if child {
                *should_break = true;
            }
            *should_break
        }
        Doc::LineSuffixBoundary => false,
    }
}
