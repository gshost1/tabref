"""hp1 (WebVoyager ArXiv--17): when was v3 of 'GPT-4 Technical Report' submitted?
Path: the paper's abstract page -> find the [v3] line of the submission history."""
import re


def run(t):
    out = t("open", "https://arxiv.org/abs/2303.08774")
    if not re.search(r"\[v3\]", out):
        out = t("find", "[v3]")
    m = re.search(r"\[v3\].{0,120}?([A-Z][a-z]{2}, \d{1,2} [A-Z][a-z]{2} \d{4})", out, re.S)
    return f"DONE v3 was submitted on {m.group(1)}" if m else "DONE"
