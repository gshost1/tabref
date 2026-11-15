use std::path::PathBuf;

use super::ProcessView;

/// `/proc/<pid>/cmdline` of a Chrome tabref launched (headless Chrome
/// rewrites its title, so the whole line is one string).
const JOINED: &str = "/opt/google/chrome/chrome --user-data-dir=/home/u/.cache/tmp/tabref-profile-sfvhem5u \
                      --remote-debugging-port=0 --no-first-run --no-default-browser-check \
                      --disable-search-engine-choice-screen --window-size=1280,900 --headless=new \
                      --noerrdialogs --ozone-platform=headless --ozone-override-screen-size=800,600 \
                      --use-angle=swiftshader-webgl about:blank";

fn view(cmdline: &[&str]) -> ProcessView {
    ProcessView {
        pid: 1,
        exe: PathBuf::from("/opt/google/chrome/chrome"),
        cmdline: cmdline.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

#[test]
fn a_command_line_rewritten_into_one_string_reads_as_its_arguments() {
    let args = view(&[JOINED]).args();
    assert_eq!(args[0], "/opt/google/chrome/chrome");
    assert_eq!(
        args[1],
        "--user-data-dir=/home/u/.cache/tmp/tabref-profile-sfvhem5u"
    );
    assert!(args.contains(&"--headless=new".to_owned()));
}

#[test]
fn separate_arguments_stay_as_they_are_even_with_spaces() {
    let args = view(&["chrome", "--user-data-dir=/tmp/a b", "about:blank"]).args();
    assert_eq!(args, ["chrome", "--user-data-dir=/tmp/a b", "about:blank"]);
}
