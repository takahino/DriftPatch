use similar::{ChangeTag, DiffOp, TextDiff};

/// a 側の削除ハイライト範囲と b 側の追加ハイライト範囲を返す（バイトオフセット）。
pub fn inline_diff(a: &str, b: &str) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    let diff = TextDiff::from_lines(a, b);
    let a_starts = line_starts(a);
    let b_starts = line_starts(b);
    let mut a_ranges = Vec::new();
    let mut b_ranges = Vec::new();

    for op in diff.ops() {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                a_ranges.push(line_range(a, &a_starts, *old_index, *old_index + *old_len));
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                b_ranges.push(line_range(b, &b_starts, *new_index, *new_index + *new_len));
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
                ..
            } => {
                let (os, oe) = line_range(a, &a_starts, *old_index, *old_index + *old_len);
                let (ns, ne) = line_range(b, &b_starts, *new_index, *new_index + *new_len);
                let (sub_a, sub_b) = word_diff_ranges(&a[os..oe], &b[ns..ne], os, ns);
                a_ranges.extend(sub_a);
                b_ranges.extend(sub_b);
            }
        }
    }

    (a_ranges, b_ranges)
}

/// 各行先頭のバイトオフセット
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, c) in text.char_indices() {
        if c == '\n' {
            starts.push(i + c.len_utf8());
        }
    }
    starts
}

/// 行 `[from, to)` のバイト範囲
fn line_range(text: &str, starts: &[usize], from: usize, to: usize) -> (usize, usize) {
    let start = starts.get(from).copied().unwrap_or(text.len());
    let end = starts.get(to).copied().unwrap_or(text.len());
    (start, end)
}

/// Replace ブロック内を単語レベルで refine する
fn word_diff_ranges(
    old_slice: &str,
    new_slice: &str,
    base_a: usize,
    base_b: usize,
) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    let diff = TextDiff::from_words(old_slice, new_slice);
    let mut a_ranges = Vec::new();
    let mut b_ranges = Vec::new();
    let mut a_offset = 0usize;
    let mut b_offset = 0usize;

    for change in diff.iter_all_changes() {
        let len = change.value().len();
        match change.tag() {
            ChangeTag::Delete => {
                a_ranges.push((base_a + a_offset, base_a + a_offset + len));
                a_offset += len;
            }
            ChangeTag::Insert => {
                b_ranges.push((base_b + b_offset, base_b + b_offset + len));
                b_offset += len;
            }
            ChangeTag::Equal => {
                a_offset += len;
                b_offset += len;
            }
        }
    }

    (a_ranges, b_ranges)
}

/// バイト範囲 `[start, end)` が highlight_ranges のいずれかと重なるか
pub fn overlaps_range(start: usize, end: usize, highlight_ranges: &[(usize, usize)]) -> bool {
    highlight_ranges
        .iter()
        .any(|&(hs, he)| start < he && end > hs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_diff() {
        let text = "hello\nworld\n";
        let (a, b) = inline_diff(text, text);
        assert!(a.is_empty());
        assert!(b.is_empty());
    }

    #[test]
    fn test_insert_line() {
        let a = "line1\nline3\n";
        let b = "line1\nline2\nline3\n";
        let (removed, added) = inline_diff(a, b);
        assert!(removed.is_empty());
        assert!(!added.is_empty());
        assert!(b[added[0].0..added[0].1].contains("line2"));
    }

    #[test]
    fn test_delete_line() {
        let a = "line1\nline2\nline3\n";
        let b = "line1\nline3\n";
        let (removed, added) = inline_diff(a, b);
        assert!(!removed.is_empty());
        assert!(added.is_empty());
        assert!(a[removed[0].0..removed[0].1].contains("line2"));
    }

    #[test]
    fn test_replace_word() {
        let a = "foo bar baz\n";
        let b = "foo qux baz\n";
        let (removed, added) = inline_diff(a, b);
        assert!(!removed.is_empty());
        assert!(!added.is_empty());
        assert!(a[removed[0].0..removed[0].1].contains("bar"));
        assert!(b[added[0].0..added[0].1].contains("qux"));
    }

    #[test]
    fn test_overlaps_range() {
        assert!(overlaps_range(5, 10, &[(7, 12)]));
        assert!(!overlaps_range(5, 7, &[(7, 12)]));
        assert!(overlaps_range(0, 100, &[(10, 20)]));
    }
}
