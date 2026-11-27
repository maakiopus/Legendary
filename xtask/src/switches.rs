//! Every `VICTORY_*` switch the documents name is read by the code (#49).
//!
//! A switch is read through a string literal of its whole name
//! (`std::env::var("VICTORY_X")`, `crate::switch::on("VICTORY_X")`,
//! `crate::ab::pick("VICTORY_X")`), so a name the Markdown files give must
//! appear quoted in at least one Rust file. The shell wrappers under
//! `tools/` and `run.sh` read their own `VICTORY_*` settings without quotes;
//! a name found anywhere in a `.sh` file counts too. xtask's own files are
//! left out of the search: this module's tests name switches of their own.

use std::collections::BTreeSet;

/// Names the documents may give although nothing here reads them, with the
/// reason each is named. They are switches of a sibling tree, or ports that
/// are still open; the documents say so at the place each appears.
const DOCUMENTED_ONLY: &[(&str, &str)] = &[
    (
        "VICTORY_ALIAS_WHY",
        "docs/ghost-fix.md §What was tried: the sibling tree's alias diagnostic",
    ),
    (
        "VICTORY_BAKE_HOST",
        "docs/ghost-fix.md §Next steps: a proposed marking, not implemented here",
    ),
    (
        "VICTORY_COMPUTE_WAIT",
        "porting-analysis.md, porting-todo.md: avatarmanz #166 is still open",
    ),
    (
        "VICTORY_COMPUTE_WAIT_MS",
        "docs/rx6700xt-linux-porting.md: named as absent, from the sibling build",
    ),
    (
        "VICTORY_KEYS_FILE",
        "docs/rx6700xt-linux-porting.md: named as absent, from the sibling build",
    ),
    (
        "VICTORY_PORT_PLACED",
        "docs/rx6700xt-linux-porting.md: the sibling tree's Vulkan layer only",
    ),
    (
        "VICTORY_UAV_RECHECK_TABLES",
        "docs/ghost-fix.md: a commit of the sibling tree, kept separate",
    ),
    (
        "VICTORY_WC_NO_BACK",
        "docs/rx6700xt-linux-porting.md: an external candidate, not implemented here",
    ),
];

/// The `VICTORY_*` names in `text`, without trailing underscores (a prefix
/// such as `VICTORY_` alone or `VICTORY_...` is not a name).
fn names(text: &str) -> BTreeSet<String> {
    const PREFIX: &str = "VICTORY_";
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find(PREFIX) {
        let tail = &rest[at + PREFIX.len()..];
        let len = tail
            .bytes()
            .take_while(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
            .count();
        let suffix = tail[..len].trim_end_matches('_');
        if !suffix.is_empty() {
            found.insert(format!("{PREFIX}{suffix}"));
        }
        rest = &tail[len..];
    }
    found
}

/// Whether `text` gives `name` as a whole name: not as the start of a
/// longer one, such as `VICTORY_LATE_WAIT` inside `VICTORY_LATE_WAITS`.
fn mentions(text: &str, name: &str) -> bool {
    let mut from = 0;
    while let Some(at) = text[from..].find(name) {
        let end = from + at + name.len();
        let longer = text[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if !longer {
            return true;
        }
        from = end;
    }
    false
}

/// `script` without its comment lines: a name a comment mentions is not a
/// name the script reads.
fn uncommented(script: &str) -> String {
    script
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The names of `documented` that no text of `sources` holds in quotes, no
/// text of `scripts` gives whole, and `DOCUMENTED_ONLY` does not explain.
fn unread<'a>(
    documented: &'a BTreeSet<String>,
    sources: &[String],
    scripts: &[String],
) -> Vec<&'a str> {
    documented
        .iter()
        .filter(|name| {
            let quoted = format!("\"{name}\"");
            let bare = name.as_str();
            !sources.iter().any(|s| s.contains(&quoted))
                && !scripts.iter().any(|s| mentions(s, bare))
                && !DOCUMENTED_ONLY.iter().any(|(known, _)| *known == bare)
        })
        .map(String::as_str)
        .collect()
}

#[test]
fn documented_switches_are_read() {
    let root = crate::workspace_root();
    let files = crate::policy::list_files(&root, false).unwrap();
    let read = |path: &String| std::fs::read_to_string(root.join(path)).ok();
    let mut documented = BTreeSet::new();
    let mut doc_of = Vec::new();
    for path in files.iter().filter(|p| p.ends_with(".md")) {
        for name in names(&read(path).unwrap_or_default()) {
            doc_of.push((name.clone(), path.clone()));
            documented.insert(name);
        }
    }
    let sources: Vec<String> = files
        .iter()
        .filter(|p| p.ends_with(".rs") && !p.starts_with("xtask/"))
        .filter_map(read)
        .collect();
    let scripts: Vec<String> = files
        .iter()
        .filter(|p| p.ends_with(".sh"))
        .filter_map(read)
        .map(|s| uncommented(&s))
        .collect();
    assert!(
        !documented.is_empty(),
        "no VICTORY_* names found in the Markdown files"
    );
    let dead = unread(&documented, &sources, &scripts);
    let report: Vec<String> = dead
        .iter()
        .map(|name| {
            let docs: Vec<&str> = doc_of
                .iter()
                .filter(|(n, _)| n == name)
                .map(|(_, p)| p.as_str())
                .collect();
            format!("{name} (named in {})", docs.join(", "))
        })
        .collect();
    assert!(
        dead.is_empty(),
        "switches the documents name but no Rust source reads: {}",
        report.join("; ")
    );
}

#[test]
fn finds_whole_names_only() {
    let text =
        "`VICTORY_LATE_WAIT=3`, VICTORY_LATE_WAITS, `VICTORY_...`, VICTORY_ alone, VICTORY_X_ end";
    let found: Vec<String> = names(text).into_iter().collect();
    assert_eq!(
        found,
        ["VICTORY_LATE_WAIT", "VICTORY_LATE_WAITS", "VICTORY_X"]
    );
}

#[test]
fn a_comment_does_not_read_a_switch() {
    let documented = names("VICTORY_LOG_DIR and VICTORY_THP");
    assert!(documented.contains("VICTORY_LOG_DIR"));
    let sources: [String; 0] = [];
    let scripts = [uncommented(
        "# VICTORY_LOG_DIR is documented only\nTHP=${VICTORY_THP:-1}",
    )];
    assert_eq!(
        unread(&documented, &sources, &scripts),
        ["VICTORY_LOG_DIR"]
    );
}

#[test]
fn a_longer_name_does_not_read_a_shorter_one() {
    let documented = names("VICTORY_LATE_WAIT and VICTORY_WIDE_BACK");
    let sources = [
        "std::env::var_os(\"VICTORY_LATE_WAITS\")".to_owned(),
        "on(\"VICTORY_WIDE_BACK\")".to_owned(),
    ];
    let scripts = ["${VICTORY_LATE_WAITS:-1} ${VICTORY_WIDE_BACK:-0}".to_owned()];
    assert_eq!(
        unread(&documented, &sources, &scripts),
        ["VICTORY_LATE_WAIT"]
    );
}
