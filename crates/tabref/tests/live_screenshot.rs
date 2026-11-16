//! `tabref screenshot` on a headless Chrome: the page or one element as a
//! PNG in the directory the command was typed in (through the daemon, which
//! runs elsewhere), clipped to an element below the fold so the clip must
//! follow the scroll, a relative `--out` taken from the client's directory,
//! an `--out` in the state directory refused with nothing written, an
//! element taller than the viewport painted to its bottom, and a tab in the
//! background refused at once. Pixels are read back with Python's zlib.
//! Live (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::{Harness, Run, harness, live, scratch, serve};

const PAGE: &str = "<!doctype html><title>Shot</title><body style='margin:0;background:#fff'>\
    <h1>Shot</h1><div style='height:1500px'></div>\
    <button style='display:block;width:120px;height:80px;margin-left:40px;border:0;\
    padding:0;background:#ff0000;color:#ff0000;font-size:1px'>Red</button>\
    <div style='height:600px'></div>\
    <button style='display:block;width:120px;height:2000px;border:0;padding:0;\
    background:#0000ff;color:#0000ff;font-size:1px'>Tall</button>\
    <div style='height:300px'></div></body>";

/// The RGB of the pixel in the PNG's middle column at the fraction
/// `sys.argv[2]` of its height, as Python reads it.
const PIXEL: &str = "import struct,sys,zlib
d=open(sys.argv[1],'rb').read();p=8;w=h=0;ct=0;idat=b''
while p<len(d):
    n=struct.unpack('>I',d[p:p+4])[0];t=d[p+4:p+8];c=d[p+8:p+8+n];p+=12+n
    if t==b'IHDR':w,h,_,ct=struct.unpack('>IIBB',c[:10])
    if t==b'IDAT':idat+=c
bpp=4 if ct==6 else 3;raw=zlib.decompress(idat);stride=w*bpp;rows=[];prev=bytearray(stride);i=0
for _ in range(h):
    f=raw[i];row=bytearray(raw[i+1:i+1+stride]);i+=1+stride
    for x in range(stride):
        a=row[x-bpp] if x>=bpp else 0;b=prev[x];c=prev[x-bpp] if x>=bpp else 0
        if f==1:row[x]=(row[x]+a)&255
        elif f==2:row[x]=(row[x]+b)&255
        elif f==3:row[x]=(row[x]+(a+b)//2)&255
        elif f==4:
            q=a+b-c;pa,pb,pc=abs(q-a),abs(q-b),abs(q-c)
            row[x]=(row[x]+(a if pa<=pb and pa<=pc else b if pb<=pc else c))&255
    rows.append(row);prev=row
r=rows[min(h-1,int(h*float(sys.argv[2])))];x=(w//2)*bpp;print(r[x],r[x+1],r[x+2])";

fn shoot(tabref: &Harness, cwd: &Path, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args([
            "--cdp",
            &tabref.browser.port.to_string(),
            "--timeout",
            "30000",
        ])
        .args(args)
        .current_dir(cwd)
        .env("TABREF_HOME", &tabref.home)
        .env_remove("TABREF_NO_DAEMON")
        .output()
        .unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn pixel_at(png: &Path, down: f64) -> String {
    let output = Command::new("python3")
        .args(["-I", "-c", PIXEL])
        .arg(png)
        .arg(down.to_string())
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn screenshots_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
        .collect()
}

#[test]
fn screenshots_land_in_the_typing_directory_clip_to_an_element_and_never_in_the_state_directory() {
    if !live() {
        return;
    }
    let tabref = harness("screenshot");
    let work = scratch("screenshot-work");
    std::fs::create_dir_all(work.0.join("shots")).unwrap();
    let url = format!("http://127.0.0.1:{}/", serve(&[("/", PAGE)]));
    tabref.ok(&["open", &url]);

    let element = shoot(&tabref, &work.0, &["--json", "screenshot", "Red"]);
    let element_files = screenshots_in(&work.0);
    let page = shoot(&tabref, &work.0, &["screenshot", "--out", "shots/page.png"]);
    let page_file = work.0.join("shots").join("page.png");
    let state = shoot(
        &tabref,
        &work.0,
        &[
            "--json",
            "screenshot",
            "--out",
            tabref.home.join("shot.png").to_str().unwrap(),
        ],
    );
    let in_state = tabref.home.join("shot.png").exists();
    let red = element_files.first().map(|file| pixel_at(file, 0.5));
    let page_center = page_file.exists().then(|| pixel_at(&page_file, 0.5));
    let _ = shoot(&tabref, &work.0, &["daemon", "stop"]);

    assert!(element.ok, "{}{}", element.stdout, element.stderr);
    let data = &element.json()["data"];
    assert_eq!(element_files.len(), 1, "{:?}", element_files);
    assert_eq!(
        data["path"].as_str().map(PathBuf::from),
        Some(std::fs::canonicalize(&element_files[0]).unwrap())
    );
    assert_eq!(
        (data["width"].as_u64(), data["height"].as_u64()),
        (Some(120), Some(80))
    );
    assert_eq!(
        red.as_deref(),
        Some("255 0 0"),
        "the clip missed the element below the fold"
    );
    assert!(page.ok, "{}{}", page.stdout, page.stderr);
    assert!(
        page.stdout.contains("page.png") && page.stdout.contains(" px, "),
        "{}",
        page.stdout
    );
    assert!(
        page_center.is_some(),
        "the relative --out was not written in the typing directory"
    );
    assert!(!state.ok, "{}", state.stdout);
    assert_eq!(
        state.json()["error"]["details"]["kind"],
        "screenshot_path_refused"
    );
    assert!(
        !in_state,
        "a screenshot was written into the state directory"
    );
}

#[test]
fn a_tall_element_is_painted_to_its_bottom_and_a_background_tab_is_refused_at_once() {
    if !live() {
        return;
    }
    let tabref = harness("screenshot-tall");
    let work = scratch("screenshot-tall-work");
    let url = format!("http://127.0.0.1:{}/", serve(&[("/", PAGE)]));
    tabref.ok(&["open", &url]);

    let tall = shoot(
        &tabref,
        &work.0,
        &["--json", "screenshot", "Tall", "--out", "tall.png"],
    );
    let bottom = work
        .0
        .join("tall.png")
        .exists()
        .then(|| pixel_at(&work.0.join("tall.png"), 0.98));
    tabref.ok(&["open", "about:blank", "--new-tab"]);
    let started = std::time::Instant::now();
    let hidden = shoot(
        &tabref,
        &work.0,
        &["--json", "--tab", "0", "screenshot", "--out", "back.png"],
    );
    let took = started.elapsed();
    let written = work.0.join("back.png").exists();
    let _ = shoot(&tabref, &work.0, &["daemon", "stop"]);

    assert!(tall.ok, "{}{}", tall.stdout, tall.stderr);
    let data = &tall.json()["data"];
    assert_eq!(
        (data["width"].as_u64(), data["height"].as_u64()),
        (Some(120), Some(2000))
    );
    assert_eq!(
        bottom.as_deref(),
        Some("0 0 255"),
        "the part of the element below the viewport was not painted"
    );
    assert!(!hidden.ok, "{}", hidden.stdout);
    let error = &hidden.json()["error"];
    assert_eq!(error["details"]["kind"], "screenshot_tab_hidden", "{error}");
    assert!(
        error["suggestion"]
            .as_str()
            .unwrap_or_default()
            .contains("tabref tab 0"),
        "{error}"
    );
    assert!(took.as_secs() < 5, "the refusal took {took:?}");
    assert!(!written);
}
