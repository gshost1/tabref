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
