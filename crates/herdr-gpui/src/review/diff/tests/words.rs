//! The words that changed within a changed line.
use super::*;

fn words(text: &str, ranges: &[Range<u32>]) -> Vec<String> {
    ranges
        .iter()
        .map(|range| text[range.start as usize..range.end as usize].to_owned())
        .collect()
}

#[test]
fn an_edited_line_marks_only_the_words_that_changed() {
    let diff = Diff::parse(
        "diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,3 +1,3 @@
-let total = price * count;
+let total = price * amount;
-fn wholly() {}
+struct Different;
 same
",
    );
    let lines = diff.files[0].lines().unwrap();
    let emphasis = emphasis(lines, 1..lines.len());
    // Each removal is paired with the addition after it.
    assert_eq!(words(lines.text(1), &emphasis[0]), ["count"]);
    assert_eq!(words(lines.text(2), &emphasis[1]), ["amount"]);
    // Lines with little in common are not an edit: nothing is marked.
    assert!(emphasis[2].is_empty());
    assert!(emphasis[3].is_empty());
    assert!(emphasis[4].is_empty());
}
