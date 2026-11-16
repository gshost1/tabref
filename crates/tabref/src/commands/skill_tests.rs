use super::run;

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
