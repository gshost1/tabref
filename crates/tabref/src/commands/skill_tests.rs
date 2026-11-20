use super::run;

#[test]
fn the_guide_explains_how_to_start_chrome_when_needed() {
    let guide = run(None).text;
    assert!(guide.contains("If no browser is connected, use `tabref open <url> --launch`."));
    assert!(!guide.contains("Chrome is already connected"));
    assert!(!guide.contains("Chrome is connected"));
}

#[test]
fn the_guide_stays_short_and_points_to_the_rare_commands() {
    let guide = run(None).text;
    assert!(guide.len() <= 2_300, "{} bytes", guide.len());
    assert!(guide.contains("tabref skill more"));
    let more = run(Some("more")).text;
    for rare in [
        "--user-chrome",
        "--profile DIR",
        "--download-dir",
        "Electron",
        "wait --download",
    ] {
        assert!(more.contains(rare), "{rare}");
        assert!(!guide.contains(rare), "{rare} belongs in skill more");
    }
}

#[test]
fn the_guide_does_not_offer_upload_as_a_batch_step() {
    let guide = run(None).text;
    let steps = guide
        .lines()
        .find(|line| line.starts_with("Steps:"))
        .unwrap_or_default();
    assert!(steps.contains("but `upload`"), "{steps}");
}

#[test]
fn the_guide_says_what_to_do_instead_of_what_not_to() {
    let guide = run(None).text;
    for rule in [
        "read that instead of taking a snapshot",
        "in one `tabref do`",
        "repeat `scroll down` in a `for`",
        "names survive a reload",
    ] {
        assert!(guide.contains(rule), "{rule}");
    }
    for misleading in [
        "not shell loops",
        "do not\nsnapshot again",
        "waits for the page itself",
    ] {
        assert!(!guide.contains(misleading), "{misleading}");
    }
}

#[test]
fn the_guide_keeps_the_forms_a_model_copies() {
    let guide = run(None).text;
    for form in [
        "across the whole page, not the first block",
        "[e9 select Size = \"M\" options: S | M | L]",
        "visible|enabled|hidden|gone|value=X",
        "for n in 1 2 3 4 5 6",
    ] {
        assert!(guide.contains(form), "{form}");
    }
    let example = guide.split("tabref do <<'EOF'").nth(1).unwrap_or_default();
    let body = example.split("\n    end").next().unwrap_or_default();
    assert!(body.contains("click \"Save\""), "{body}");
}

#[test]
fn the_guide_keeps_error_recovery_hints() {
    let guide = run(None).text;
    for hint in [
        "`ELEMENT_NOT_FOUND`: check close names.",
        "`AMBIGUOUS_TARGET`: add `nth N`, `in` or `after`.",
        "`STALE_REF`: use the fresh ref.",
    ] {
        assert!(guide.contains(hint), "{hint}");
    }
}
