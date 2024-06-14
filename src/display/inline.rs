//! Inline, or "unified" diff display.

use line_numbers::LineNumber;

use crate::{
    constants::Side,
    display::{
        context::{calculate_after_context, calculate_before_context, opposite_positions},
        hunks::Hunk,
        style::{self, apply_colors, apply_line_number_color},
    },
    lines::{format_line_num, format_line_num_padded, split_on_newlines, MaxLine},
    options::DisplayOptions,
    parse::syntax::MatchedPos,
    summary::FileFormat,
};

pub(crate) fn print(
    lhs_src: &str,
    rhs_src: &str,
    display_options: &DisplayOptions,
    lhs_positions: &[MatchedPos],
    rhs_positions: &[MatchedPos],
    hunks: &[Hunk],
    display_path: &str,
    extra_info: &Option<String>,
    file_format: &FileFormat,
) {
    let (lhs_colored_lines, rhs_colored_lines) = if display_options.use_color {
        (
            apply_colors(
                lhs_src,
                Side::Left,
                display_options.syntax_highlight,
                file_format,
                display_options.background_color,
                lhs_positions,
            ),
            apply_colors(
                rhs_src,
                Side::Right,
                display_options.syntax_highlight,
                file_format,
                display_options.background_color,
                rhs_positions,
            ),
        )
    } else {
        (
            split_on_newlines(lhs_src)
                .map(|s| format!("{}\n", s))
                .collect(),
            split_on_newlines(rhs_src)
                .map(|s| format!("{}\n", s))
                .collect(),
        )
    };

    let lhs_colored_lines: Vec<_> = lhs_colored_lines
        .into_iter()
        .map(|line| style::replace_tabs(&line, display_options.tab_width))
        .collect();
    let rhs_colored_lines: Vec<_> = rhs_colored_lines
        .into_iter()
        .map(|line| style::replace_tabs(&line, display_options.tab_width))
        .collect();

    let opposite_to_lhs = opposite_positions(lhs_positions);
    let opposite_to_rhs = opposite_positions(rhs_positions);

    for (i, hunk) in hunks.iter().enumerate() {
        println!(
            "{}",
            style::header(
                display_path,
                extra_info.as_ref(),
                i + 1,
                hunks.len(),
                file_format,
                display_options
            )
        );

        let hunk_lines = &hunk.lines;

        let before_lines = calculate_before_context(
            hunk_lines,
            &opposite_to_lhs,
            &opposite_to_rhs,
            display_options.num_context_lines as usize,
        );
        let after_lines = calculate_after_context(
            &[&before_lines[..], &hunk_lines[..]].concat(),
            &opposite_to_lhs,
            &opposite_to_rhs,
            // TODO: repeatedly calculating the maximum is wasteful.
            lhs_src.max_line(),
            rhs_src.max_line(),
            display_options.num_context_lines as usize,
        );

        // Common context lines will be emitted once at first or last. Uncommon
        // lines will be inserted in between. Missing lines towards the hunk
        // will also be filled.
        let first_rhs_line = {
            let common_len = before_lines
                .iter()
                .take_while(|(lhs_line, rhs_line)| lhs_line.is_some() && rhs_line.is_some())
                .count();
            let (common_lines, uncommon_lines) = before_lines.split_at(common_len);
            if let Some((_, rhs_line)) = uncommon_lines.first() {
                *rhs_line // first uncommon
            } else if let Some(&(_, Some(LineNumber(a)))) = common_lines.last() {
                match to_rhs_iter(hunk_lines).next() {
                    Some(LineNumber(b)) => (a..=b).map(LineNumber).nth(1), // next of common
                    None => None,
                }
            } else {
                None
            }
        };
        let last_lhs_line = {
            let common_len = after_lines
                .iter()
                .rev()
                .take_while(|(lhs_line, rhs_line)| lhs_line.is_some() && rhs_line.is_some())
                .count();
            let (uncommon_lines, common_lines) =
                after_lines.split_at(after_lines.len() - common_len);
            if let Some((lhs_line, _)) = uncommon_lines.last() {
                *lhs_line // last uncommon
            } else if let Some(&(Some(LineNumber(b)), _)) = common_lines.first() {
                match to_lhs_iter(hunk_lines).next_back() {
                    Some(LineNumber(a)) => (a..=b).map(LineNumber).nth_back(1), // prev of common
                    None => None,
                }
            } else {
                None
            }
        };

        let all_lhs_lines = itertools::chain!(
            to_lhs_iter(&before_lines),
            to_lhs_iter(hunk_lines),
            last_lhs_line,
        );
        let all_rhs_lines = itertools::chain!(
            first_rhs_line,
            to_rhs_iter(hunk_lines),
            to_rhs_iter(&after_lines),
        );
        let first_last_lhs_lines = get_first_last(all_lhs_lines);
        let first_last_rhs_lines = get_first_last(all_rhs_lines);

        // Use the same column width so that left/right sides are aligned.
        let max_line = [first_last_lhs_lines, first_last_rhs_lines]
            .into_iter()
            .flatten()
            .map(|(_, last)| last)
            .max()
            .unwrap_or(LineNumber(0));
        let line_column_width = format_line_num(max_line).len();

        if let Some((first, last)) = first_last_lhs_lines {
            let mut lhs_hunk_lines = to_lhs_iter(hunk_lines).fuse().peekable();
            for lhs_line in (first.0..=last.0).map(LineNumber) {
                let is_novel = lhs_hunk_lines.next_if_eq(&lhs_line).is_some();
                print!(
                    "{}   {}",
                    apply_line_number_color(
                        &format_line_num_padded(lhs_line, line_column_width),
                        is_novel,
                        Side::Left,
                        display_options,
                    ),
                    lhs_colored_lines[lhs_line.as_usize()]
                );
            }
        }

        if let Some((first, last)) = first_last_rhs_lines {
            let mut rhs_hunk_lines = to_rhs_iter(hunk_lines).fuse().peekable();
            for rhs_line in (first.0..=last.0).map(LineNumber) {
                let is_novel = rhs_hunk_lines.next_if_eq(&rhs_line).is_some();
                print!(
                    "   {}{}",
                    apply_line_number_color(
                        &format_line_num_padded(rhs_line, line_column_width),
                        is_novel,
                        Side::Right,
                        display_options,
                    ),
                    rhs_colored_lines[rhs_line.as_usize()]
                );
            }
        }

        println!();
    }
}

fn to_lhs_iter<T: Copy>(
    items: &[(Option<T>, Option<T>)],
) -> impl DoubleEndedIterator<Item = T> + '_ {
    items.iter().filter_map(|(lhs, _)| *lhs)
}

fn to_rhs_iter<T: Copy>(
    items: &[(Option<T>, Option<T>)],
) -> impl DoubleEndedIterator<Item = T> + '_ {
    items.iter().filter_map(|(_, rhs)| *rhs)
}

fn get_first_last<T: Copy>(mut iter: impl DoubleEndedIterator<Item = T>) -> Option<(T, T)> {
    let first = iter.next()?;
    let last = iter.next_back().unwrap_or(first);
    Some((first, last))
}
