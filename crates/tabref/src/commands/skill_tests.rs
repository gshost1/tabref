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
    assert!(guide.len() <= 2_800, "{} bytes", guide.len());
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
