use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::{Duration, Instant};

pub fn hold(path: &Path) -> File {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1 | 2);
    }
    loop {
        match options.open(path) {
            Ok(file) => return file,
            Err(error)
                if cfg!(windows)
                    && error.raw_os_error() == Some(32)
                    && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => panic!("could not hold {}: {error}", path.display()),
        }
    }
}
