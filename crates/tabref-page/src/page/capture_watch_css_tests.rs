use super::classify;

#[test]
fn safe_sheets_cover_static_rules_and_tracked_media() {
    for css in [
        "",
        "/* @container {} */",
        "button {content:'@container :has() animation-timeline'}",
        "@charset \"UTF-8\"; @namespace svg url(http://www.w3.org/2000/svg); svg|a {color:red}",
        "@font-face{font-family:'Example';src:url(https://example.com/font.woff2);unicode-range:U+0000-00FF}",
        "@media screen and (min-width: 30em) and (orientation: landscape){a{color:red}}",
        "@media print, all and (height >= 100px){a{color:red}}",
        "@media (aspect-ratio:16/9), (max-aspect-ratio:2/1){a{color:red}}",
        "@media (prefers-color-scheme:dark) and (prefers-reduced-motion:reduce){a{color:red}}",
        "@media (prefers-contrast:custom) or (forced-colors:active){a{color:red}}",
        "@supports (display:grid){@media (max-height:10.5rem){a{display:grid}}}",
        "@layer base, theme; @layer theme { @supports selector(:has(a)) { a{display:block} } }",
        "@layer { @media screen { button { color:red } } }",
        "@MEDIA SCREEN AND (MIN-WIDTH: 10PX) { a { color:red } }",
        "a/*:has()*/[title=':has(@container)']{content:'@property'}",
    ] {
        assert!(classify("safe", css).is_some(), "{css}");
    }
}

#[test]
fn unsafe_and_unknown_constructs_are_blind_at_every_depth() {
    for css in [
        "@container(width>1px){a{color:red}}",
        "a:has(b){color:red}",
        "a:HAS(b){color:red}",
        "a:/**/has(b){color:red}",
        "@scope(.a){a{color:red}}",
        "@property --x{syntax:'*';inherits:true}",
        "@unknown foo;",
        "@unknown{a{color:red}}",
        "@starting-style{a{opacity:0}}",
        "@keyframes x{from{opacity:0}to{opacity:1}}",
        "@page{size:A4}",
        "a{animation-timeline:scroll()}",
        "a{scroll-timeline-name:--x}",
        "a{view-timeline:--x}",
        "a{timeline-scope:--x}",
        "@font-face{animation-timeline:--x}",
        "@layer theme{@supports(display:grid){@media(min-width:1px){a:has(b){color:red}}}}",
        "@supports(display:grid){@media(min-width:1px){@container(width>1px){a{color:red}}}}",
        "@media(hover:hover){a{color:red}}",
        "@media(min-resolution:2dppx){a{color:red}}",
        "@media(device-width:100px){a{color:red}}",
        "@media(color-gamut:p3){a{color:red}}",
        "@media(scripting:enabled){a{color:red}}",
        "@media(min-width:1cqw){a{color:red}}",
        "a{&:has(b){color:red}}",
        "a{@container(width>1px){color:red}}",
        "a:\\68 as(b){color:red}",
        "@\\63 ontainer(width>1px){a{color:red}}",
        "a{content:'unterminated}",
        "/* unterminated",
        "a{color:red",
        "a{color:red}}",
        "a{color:rgb(1,2,3}",
        "a{color:rgb(1,2,3))}",
        "a{--x:[1,2)}",
        "@media (width:1px {a{color:red}}",
        "@media(hover:hover);",
        "@import 'x' layer(foo);",
    ] {
        assert_eq!(classify("blind", css), None, "{css}");
    }
}

#[test]
fn imports_are_returned_for_recursive_resolution() {
    for (css, urls) in [
        ("@import 'a.css';", vec!["a.css"]),
        (
            "@import url(../a.css) screen and (min-width:1px);",
            vec!["../a.css"],
        ),
        (
            "@import url('a.css'); @import \"b.css\" print;",
            vec!["a.css", "b.css"],
        ),
        (
            "@supports(display:grid){@media screen{@import 'nested.css';}}",
            vec!["nested.css"],
        ),
    ] {
        assert_eq!(
            classify("imports", css),
            Some(urls.into_iter().map(str::to_owned).collect()),
            "{css}"
        );
    }
}

#[test]
fn same_url_and_length_do_not_reuse_a_different_texts_classification() {
    let safe = "a{color:red}                 ";
    let blind = "a{animation-timeline:scroll()}";
    let safe = format!("{safe:width$}", width = blind.len());
    assert_eq!(safe.len(), blind.len());
    assert!(classify("collision", &safe).is_some());
    assert_eq!(classify("collision", blind), None);
    assert!(classify("collision", &safe).is_some());
}

#[test]
fn excessive_input_and_nesting_fail_closed() {
    assert_eq!(classify("rules", &"a{}".repeat(10_001)), None);
    assert_eq!(classify("large", &" ".repeat(1_000_001)), None);
    let nested = format!(
        "{}a{{color:red}}{}",
        "@media screen{".repeat(18),
        "}".repeat(18)
    );
    assert_eq!(classify("nested", &nested), None);
}

#[test]
fn malformed_declarations_are_blind() {
    for css in [
        "a{color red}",
        "a{:red}",
        "a{color:}",
        "a{color:red; broken}",
        "a{color:red!important!important}",
    ] {
        assert_eq!(classify("malformed", css), None, "{css}");
    }
}

#[test]
fn tracked_state_selectors_are_safe() {
    for css in [
        "a:hover{visibility:hidden}",
        "input:checked+a{display:none}",
        "a:focus{opacity:0}",
    ] {
        assert!(classify("states", css).is_some(), "{css}");
    }
}
