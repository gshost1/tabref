use tabref_page::{Block, BlockKind, FrameSkipReason, FrameStats, PageOutline, SkippedFrame};

use super::frames_line;

fn outline(frames: FrameStats) -> PageOutline {
    PageOutline {
        body: Block::new(BlockKind::Frame),
        dialogs: Vec::new(),
        frames,
    }
}

fn skipped(reason: FrameSkipReason, out_of_process: bool) -> SkippedFrame {
    SkippedFrame {
        frame_id: "F".into(),
        url: String::new(),
        reason,
        out_of_process,
    }
}

#[test]
fn an_out_of_process_frame_that_timed_out_reads_as_not_read_in_time() {
    let page = outline(FrameStats {
        cross_origin: 3,
        tiny: 1,
        ..FrameStats::default()
    });
    let left_out = [
        skipped(FrameSkipReason::TimedOut, true),
        skipped(FrameSkipReason::Tiny, true),
        skipped(FrameSkipReason::TimedOut, false),
    ];

    assert_eq!(
        frames_line(&page, &left_out).as_deref(),
        Some(
            "frames: 4 not shown: 2 cross-origin (ads, widgets), 1 tiny or hidden, 1 not read in time"
        )
    );
}

#[test]
fn nothing_left_out_prints_no_line() {
    assert_eq!(frames_line(&outline(FrameStats::default()), &[]), None);
}

#[test]
fn a_frame_left_unread_makes_the_view_incomplete_and_the_footer_says_so() {
    use super::super::cut_report::CutReport;
    use super::super::cut_stats::CutStats;
    use super::super::row_window::RowWindow;
    use super::super::view_options::ViewOptions;
    use super::{footer_line, is_view_complete};

    let report = CutReport {
        lines_total: 3,
        lines_shown: 3,
        chars_total: 90,
        chars_shown: 90,
        ..CutReport::default()
    };
    let options = ViewOptions::snap("http://site.test/", false, RowWindow::default());
    let same_origin_late = [skipped(FrameSkipReason::TimedOut, false)];
    let ad_late = [skipped(FrameSkipReason::TimedOut, true)];

    assert!(is_view_complete(&report, &[]));
    assert!(!is_view_complete(&report, &same_origin_late));
    assert!(!is_view_complete(&report, &ad_late));
    assert!(!is_view_complete(
        &report,
        &[skipped(FrameSkipReason::Unreadable, true)]
    ));
    assert!(is_view_complete(
        &report,
        &[skipped(FrameSkipReason::Tiny, true)]
    ));

    let all = footer_line(&report, &CutStats::default(), &[], &options, 0);
    assert!(all.ends_with("all shown"), "{all}");
    let late = footer_line(&report, &CutStats::default(), &[], &options, 1);
    assert!(late.contains("1 frames not read in time"), "{late}");
    assert!(!late.contains("all shown"), "{late}");
}
