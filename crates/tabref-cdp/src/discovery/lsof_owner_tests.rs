use super::parse_lsof_pids;

#[test]
fn every_listening_process_is_read_once() {
    assert_eq!(parse_lsof_pids("p123\np123\np456\n"), [123, 456]);
    assert_eq!(parse_lsof_pids(""), Vec::<u32>::new());
    assert_eq!(parse_lsof_pids("f12\npx\np7\n"), [7]);
}
